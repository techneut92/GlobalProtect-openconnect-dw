//! Server-side authentication handoff (wire-protocol v3).
//!
//! The GUI hands the backend a [`ProbeRequest`] / [`ConnectAuthRequest`]
//! instead of doing the portal HTTP itself. The backend owns the whole flow —
//! prelogin (including the PKCS#11 mTLS), gateway login, and starting the
//! tunnel — so a GUI that links none of this (gp-client) can still connect.
//!
//! This mirrors the proven client-side flow in the fork's `apps/gpgui`
//! (`connect.rs`), moved into the service.

use anyhow::{bail, Context};
use gpapi::{
  auth::SamlAuthData,
  credential::{Credential, PasswordCredential},
  gateway::{gateway_login, GatewayLogin},
  gp_params::{ClientOs, GpParams},
  portal::{prelogin, Prelogin},
  service::request::{AuthCredential, ConnectAuthRequest, ConnectRequest, ProbeReply, ProbeRequest},
};
use gpapi::service::vpn_state::ConnectInfo;
use gp_protocol::Gateway;
use log::info;

/// Rebuild the `GpParams` the portal HTTP needs from a probe/connect request.
fn gp_params(
  certificate: Option<String>,
  sslkey: Option<String>,
  key_password: Option<String>,
  ignore_tls_errors: bool,
  os: Option<ClientOs>,
  os_version: Option<String>,
  user_agent: Option<String>,
) -> GpParams {
  let mut builder = GpParams::builder();
  if let Some(ua) = user_agent.as_deref() {
    builder.user_agent(ua);
  }
  if let Some(os) = os {
    builder.client_os(os);
  }
  if let Some(v) = os_version {
    builder.os_version(v);
  }
  builder
    .ignore_tls_errors(ignore_tls_errors)
    .certificate(certificate)
    .sslkey(sslkey)
    .key_password(key_password);

  let mut params = builder.build();
  // Direct-gateway flow: the server is treated as the gateway.
  params.set_is_gateway(true);
  params
}

/// Run prelogin and report which authentication the server wants. The mTLS
/// client cert (PKCS#11) is exercised here — a `cert_needed` error means the
/// server rejected the TLS handshake or demanded a cert we didn't present.
pub async fn probe(req: &ProbeRequest) -> ProbeReply {
  let params = gp_params(
    req.certificate.clone(),
    req.sslkey.clone(),
    req.key_password.clone(),
    req.ignore_tls_errors,
    req.os.clone(),
    req.os_version.clone(),
    req.user_agent.clone(),
  );

  match prelogin(&req.server, &params).await {
    Ok(Prelogin::Saml(saml)) => ProbeReply::Saml {
      saml_request: saml.saml_request().to_string(),
      supports_browser: saml.support_default_browser(),
    },
    Ok(Prelogin::Standard(standard)) => ProbeReply::Standard {
      username_label: standard.label_username().to_string(),
      password_label: standard.label_password().to_string(),
    },
    Err(err) => {
      let message = err.to_string();
      let cert_needed = message.to_lowercase().contains("certificate");
      ProbeReply::Error { message, cert_needed }
    }
  }
}

/// Build the gateway `Credential` from the GUI-supplied auth result.
///
/// For `CertOnly` we don't yet know the exact credential a pure-cert gateway
/// expects — that depends on what its prelogin returns — so we run prelogin and
/// surface the answer as an error the GUI can act on (fall into the password or
/// SAML flow). This is intentionally explicit until verified against a real
/// cert-auth gateway.
async fn resolve_credential(req: &ConnectAuthRequest, params: &GpParams) -> anyhow::Result<Credential> {
  match &req.credential {
    AuthCredential::Password { username, password } => Ok(PasswordCredential::new(username, password).into()),

    AuthCredential::Saml {
      username,
      prelogin_cookie,
      portal_userauthcookie,
    } => {
      let data = SamlAuthData::new(
        Some(username.clone()),
        prelogin_cookie.clone(),
        portal_userauthcookie.clone(),
      )
      .context("invalid SAML auth data")?;
      Credential::try_from(data).context("could not build a credential from the SAML result")
    }

    AuthCredential::CertOnly => match prelogin(&req.server, params).await? {
      Prelogin::Standard(_) => bail!(
        "This gateway asks for a username and password — probe it and provide credentials \
         (cert-only login for standard-auth gateways is not implemented yet)"
      ),
      Prelogin::Saml(_) => bail!(
        "This gateway uses SAML SSO — run the sign-in flow and pass the resulting cookie \
         (cert-only cannot complete SAML on its own)"
      ),
    },
  }
}

/// Full connect: prelogin context → credential → gateway login → build a
/// [`ConnectRequest`] with the resulting cookie. The caller feeds the request
/// to the existing tunnel path (`VpnTaskContext::connect`), so all state
/// broadcasting is unchanged.
pub async fn build_connect_request(req: &ConnectAuthRequest) -> anyhow::Result<ConnectRequest> {
  let params = gp_params(
    req.certificate.clone(),
    req.sslkey.clone(),
    req.key_password.clone(),
    req.ignore_tls_errors,
    req.os.clone(),
    req.os_version.clone(),
    req.user_agent.clone(),
  );

  let cred = resolve_credential(req, &params).await?;

  info!("Performing gateway login for {}", req.server);
  let cookie = match gateway_login(&req.server, &cred, &params)
    .await
    .context("gateway login failed")?
  {
    GatewayLogin::Cookie(cookie) => cookie,
    GatewayLogin::Mfa(..) => bail!("This gateway requires an MFA prompt, which is not supported yet"),
  };

  // Gateway mode: the server is its own only gateway.
  let gateway = Gateway::new(req.server.clone(), req.server.clone());
  let info = ConnectInfo::new(req.server.clone(), gateway.clone(), vec![gateway]);

  // The GUI's `args` carry the tunnel options; the cookie is filled in here.
  let a = &req.args;
  let mut request = ConnectRequest::new(info, cookie)
    .with_certificate(req.certificate.clone())
    .with_sslkey(req.sslkey.clone())
    .with_key_password(req.key_password.clone())
    .with_user_agent(req.user_agent.clone())
    .with_os(req.os.clone())
    .with_os_version(req.os_version.clone())
    .with_mtu(a.mtu())
    .with_disable_ipv6(a.disable_ipv6())
    .with_no_dtls(a.no_dtls())
    .with_no_xmlpost(a.no_xmlpost())
    .with_force_dpd(a.force_dpd())
    .with_hip(a.hip())
    .with_allow_extend_session(a.allow_extend_session());

  if a.reconnect_timeout() > 0 {
    request = request.with_reconnect_timeout(a.reconnect_timeout());
  }
  if let Some(script) = a.vpnc_script() {
    request = request.with_vpnc_script(Some(script));
  }
  if let Some(host) = a.local_hostname() {
    request = request.with_local_hostname(Some(host));
  }
  if let Some(v) = a.client_version() {
    request = request.with_client_version(&v);
  }

  Ok(request)
}
