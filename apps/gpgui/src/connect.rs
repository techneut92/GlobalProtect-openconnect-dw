//! v2 connect path — the authentication half.
//!
//! Runs portal/gateway **prelogin + SAML SSO unprivileged** (so the `gpauth`
//! webview inherits this process's display/D-Bus), then assembles the
//! `ConnectRequest` that the root `gpservice` needs to drive the openconnect
//! tunnel. Mirrors the auth pipeline in `apps/gpclient/src/connect.rs`, minus
//! the tunnel — that's gpservice's job.

use anyhow::{bail, Context, Result};
use gpapi::{
  credential::{Credential, PasswordCredential},
  gateway::{gateway_login, Gateway, GatewayLogin},
  gp_params::{ClientOs, GpParams},
  portal::{prelogin, Prelogin},
  process::auth_launcher::SamlAuthLauncher,
  service::{request::ConnectRequest, vpn_state::ConnectInfo},
  utils::host_utils,
};

/// Inputs captured from the UI for a connection attempt.
pub struct AuthParams {
  /// Server to authenticate against. Currently only the gateway flow is
  /// supported, so this is treated as the gateway.
  pub server: String,
  pub os: String,
  pub user_agent: String,
  /// Client certificate: either a pkcs11 URI **including** `?pin-value=…`, or a
  /// path to a PEM/PKCS#12 cert file. Used for the mTLS prelogin here and passed
  /// through to gpservice for the tunnel.
  pub certificate: String,
  /// Private key file (for PEM cert-file auth where the key is separate).
  pub sslkey: Option<String>,
  /// Passphrase for an encrypted key / PKCS#12 file.
  pub key_password: Option<String>,
  /// Standard (non-SSO) username/password, when the user picks that method.
  pub username: Option<String>,
  pub password: Option<String>,
  /// Run SAML in the system browser instead of the embedded webview.
  pub use_browser: bool,
  /// Advanced connection options (from the settings window).
  pub opts: ConnOpts,
}

/// Advanced connection/tunnel options (the gpclient CLI surface).
#[derive(Debug, Clone, Default)]
pub struct ConnOpts {
  pub mtu: u32,
  pub reconnect_timeout: u32,
  pub force_dpd: u32,
  pub disable_ipv6: bool,
  pub no_dtls: bool,
  pub no_xmlpost: bool,
  pub ignore_tls_errors: bool,
  /// Empty string = unset.
  pub vpnc_script: String,
  pub local_hostname: String,
  pub os_version: String,
  pub client_version: String,
}

/// What the portal's prelogin asks for.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
  /// "saml" | "standard" | "cert" (client cert required) | "error".
  pub kind: String,
  pub supports_browser: bool,
  pub username_label: String,
  pub password_label: String,
  pub message: String,
}

/// Probe a portal/gateway's prelogin to discover the required auth. Pass a
/// certificate if the server needs one (a prior probe returned `kind == "cert"`).
pub async fn probe(
  server: &str,
  os: &str,
  user_agent: &str,
  certificate: Option<String>,
  sslkey: Option<String>,
  key_password: Option<String>,
  ignore_tls_errors: bool,
) -> ProbeResult {
  let client_os = ClientOs::from(os);
  let mut builder = GpParams::builder();
  builder
    .user_agent(user_agent)
    .client_os(client_os)
    .os_version(os_version(&ClientOs::from(os)))
    .ignore_tls_errors(ignore_tls_errors)
    .certificate(certificate)
    .sslkey(sslkey)
    .key_password(key_password);
  let mut gp_params = builder.build();
  gp_params.set_is_gateway(true);

  let blank = || (String::new(), String::new());
  match prelogin(server, &gp_params).await {
    Ok(Prelogin::Saml(s)) => {
      let (username_label, password_label) = blank();
      ProbeResult {
        kind: "saml".into(),
        supports_browser: s.support_default_browser(),
        username_label,
        password_label,
        message: String::new(),
      }
    }
    Ok(Prelogin::Standard(s)) => ProbeResult {
      kind: "standard".into(),
      supports_browser: false,
      username_label: s.label_username().to_string(),
      password_label: s.label_password().to_string(),
      message: String::new(),
    },
    Err(e) => {
      let msg = e.to_string();
      let kind = if msg.to_lowercase().contains("certificate") { "cert" } else { "error" };
      let (username_label, password_label) = blank();
      ProbeResult { kind: kind.into(), supports_browser: false, username_label, password_label, message: msg }
    }
  }
}

/// Default OS version string matching what gpclient sends for the given OS.
fn os_version(os: &ClientOs) -> String {
  match os {
    ClientOs::Linux => host_utils::get_linux_os_string(),
    ClientOs::Windows => host_utils::get_windows_os_string(),
    ClientOs::Mac => host_utils::get_macos_os_string(),
  }
  .to_string()
}

/// Authenticate (prelogin → SAML → gateway login) and build the
/// `ConnectRequest` for gpservice. Runs as the unprivileged GUI user.
pub async fn build_connect_request(p: &AuthParams) -> Result<ConnectRequest> {
  let o = &p.opts;
  let client_os = ClientOs::from(p.os.as_str());
  // Allow an explicit OS-version override from the advanced options.
  let os_version = if o.os_version.is_empty() {
    os_version(&client_os)
  } else {
    o.os_version.clone()
  };

  // No certificate for the SAML/SSO and username-password methods.
  let certificate = (!p.certificate.is_empty()).then(|| p.certificate.clone());

  let mut builder = GpParams::builder();
  builder
    .user_agent(&p.user_agent)
    .client_os(ClientOs::from(p.os.as_str()))
    .os_version(os_version.clone())
    .ignore_tls_errors(o.ignore_tls_errors)
    .certificate(certificate)
    .sslkey(p.sslkey.clone())
    .key_password(p.key_password.clone());
  if !o.local_hostname.is_empty() {
    builder.computer(&o.local_hostname);
  }
  let mut gp_params = builder.build();
  gp_params.set_is_gateway(true);

  // 1. Prelogin — this is the mTLS step that uses the pkcs11 client cert.
  let prelogin = prelogin(&p.server, &gp_params)
    .await
    .context("portal prelogin failed — check the server address and client certificate")?;

  // 2. Obtain the credential.
  //  - username/password supplied → standard credential (no SSO).
  //  - else SAML: launches gpauth as THIS user (display present, no pkexec).
  let cred: Credential = if let (Some(u), Some(pw)) = (p.username.as_deref(), p.password.as_deref()) {
    PasswordCredential::new(u, pw).into()
  } else {
    match &prelogin {
      Prelogin::Saml(saml) => {
        SamlAuthLauncher::new(&p.server)
          // gpauth is bundled at /app/bin under Flatpak; the default is /usr/bin.
          .auth_executable(crate::system::is_flatpak().then_some("/app/bin/gpauth"))
          .gateway(true)
          .saml_request(saml.saml_request())
          .user_agent(&p.user_agent)
          .os(p.os.as_str())
          .os_version(Some(&os_version))
          .ignore_tls_errors(o.ignore_tls_errors)
          .default_browser(p.use_browser)
          .launch()
          .await
          .context("single sign-on was cancelled or failed")?
      }
      Prelogin::Standard(_) => {
        bail!("This server needs a username and password — choose the \"Username & password\" method")
      }
    }
  };

  // 3. Gateway login → the auth cookie that gpservice feeds to openconnect.
  let cookie = match gateway_login(&p.server, &cred, &gp_params)
    .await
    .context("gateway login failed")?
  {
    GatewayLogin::Cookie(cookie) => cookie,
    GatewayLogin::Mfa(..) => {
      bail!("This gateway requires an MFA prompt, which the GUI doesn't support yet")
    }
  };

  // 4. Assemble the ConnectRequest. In gateway mode the server is itself the
  //    only gateway.
  let gateway = Gateway::new(p.server.clone(), p.server.clone());
  let info = ConnectInfo::new(p.server.clone(), gateway.clone(), vec![gateway]);
  let mut request = ConnectRequest::new(info, cookie)
    .with_certificate((!p.certificate.is_empty()).then(|| p.certificate.clone()))
    .with_sslkey(p.sslkey.clone())
    .with_key_password(p.key_password.clone())
    .with_user_agent(Some(p.user_agent.clone()))
    .with_os(Some(client_os))
    .with_os_version(Some(os_version))
    .with_mtu(o.mtu)
    .with_disable_ipv6(o.disable_ipv6)
    .with_no_dtls(o.no_dtls)
    .with_no_xmlpost(o.no_xmlpost)
    .with_force_dpd(o.force_dpd);

  if o.reconnect_timeout > 0 {
    request = request.with_reconnect_timeout(o.reconnect_timeout);
  }
  if !o.vpnc_script.is_empty() {
    request = request.with_vpnc_script(Some(o.vpnc_script.clone()));
  }
  if !o.local_hostname.is_empty() {
    request = request.with_local_hostname(Some(o.local_hostname.clone()));
  }
  if !o.client_version.is_empty() {
    request = request.with_client_version(&o.client_version);
  }

  Ok(request)
}
