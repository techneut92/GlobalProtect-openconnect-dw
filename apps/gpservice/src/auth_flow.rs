//! Server-side authentication handoff (wire-protocol v3).
//!
//! The GUI hands the backend a [`ProbeRequest`] / [`ConnectAuthRequest`]
//! instead of doing the portal HTTP itself. The backend owns the whole flow —
//! prelogin (including the PKCS#11 mTLS), gateway login, and starting the
//! tunnel — so a GUI that links none of this (gp-client) can still connect.
//!
//! This mirrors the proven client-side prelogin/gateway-login flow, moved into
//! the service so the (webkit-free) GUI never links it.

use anyhow::{bail, Context};
use gpapi::{
  auth::SamlAuthData,
  credential::{Credential, PasswordCredential},
  gateway::{gateway_login, GatewayLogin},
  gp_params::{ClientOs, GpParams},
  portal::{prelogin, retrieve_config, PortalConfig, PortalConfigResult, Prelogin},
  service::request::{AuthCredential, ConnectAuthRequest, ConnectRequest, ProbeReply, ProbeRequest},
};
use gpapi::service::vpn_state::{ConnectInfo, MfaChallengeInfo, VpnState};
use gp_protocol::Gateway;
use log::info;
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, watch};

/// Parked interactive-MFA request: the connect flow stores a oneshot here while
/// the GUI is prompted for the code; `submit_mfa` resolves it with the code, or
/// `disconnect` resolves it with `None` (cancel).
pub type MfaSlot = Arc<Mutex<Option<oneshot::Sender<Option<String>>>>>;

/// Emits the `MfaChallenge` state to the GUI and awaits the entered one-time
/// code. Mirrors the GUI-side PIN prompt bridge, one hop further back.
pub struct MfaPrompter {
  vpn_state_tx: Arc<watch::Sender<VpnState>>,
  slot: MfaSlot,
}

impl MfaPrompter {
  pub fn new(vpn_state_tx: Arc<watch::Sender<VpnState>>, slot: MfaSlot) -> Self {
    Self { vpn_state_tx, slot }
  }

  /// Show the challenge and wait for the code; `None` if the user cancelled.
  async fn prompt(&self, message: String) -> Option<String> {
    let (tx, rx) = oneshot::channel::<Option<String>>();
    *self.slot.lock().unwrap() = Some(tx);
    self
      .vpn_state_tx
      .send(VpnState::MfaChallenge(Box::new(MfaChallengeInfo::new(message))))
      .ok();
    rx.await.unwrap_or(None)
  }
}

/// Parked gateway-selection request: the portal connect flow stores a oneshot
/// here while the GUI shows the gateway picker; `select_gateway` resolves it
/// with the chosen gateway's address, or `disconnect` with `None` (cancel).
pub type GatewaySlot = Arc<Mutex<Option<oneshot::Sender<Option<String>>>>>;

/// Emits the `GatewaySelect` state to the GUI and awaits the chosen gateway.
/// Mirrors [`MfaPrompter`] for the portal's multi-gateway case.
pub struct GatewayPrompter {
  vpn_state_tx: Arc<watch::Sender<VpnState>>,
  slot: GatewaySlot,
}

impl GatewayPrompter {
  pub fn new(vpn_state_tx: Arc<watch::Sender<VpnState>>, slot: GatewaySlot) -> Self {
    Self { vpn_state_tx, slot }
  }

  /// Show the picker (preferred pre-selected, full list attached) and wait for
  /// the chosen gateway address; `None` if the user cancelled.
  async fn prompt(&self, portal: String, preferred: Gateway, gateways: Vec<Gateway>) -> Option<String> {
    let (tx, rx) = oneshot::channel::<Option<String>>();
    *self.slot.lock().unwrap() = Some(tx);
    self
      .vpn_state_tx
      .send(VpnState::GatewaySelect(Box::new(ConnectInfo::new(
        portal, preferred, gateways,
      ))))
      .ok();
    rx.await.unwrap_or(None)
  }
}

/// Gateway login that answers interactive MFA / token challenges: on a
/// `Challenge`, prompt the GUI, then resubmit with the entered code + the
/// challenge token until the gateway returns a cookie (or the user cancels).
async fn gateway_login_mfa(
  server: &str,
  cred: &Credential,
  params: &GpParams,
  mfa: &MfaPrompter,
) -> anyhow::Result<String> {
  let mut result = gateway_login(server, cred, params).await.context("gateway login failed")?;
  loop {
    match result {
      GatewayLogin::Cookie(cookie) => return Ok(cookie),
      GatewayLogin::Mfa(message, input_str) => {
        info!("Gateway issued an MFA challenge; prompting the user for a code");
        let code = mfa.prompt(message).await.context("MFA challenge cancelled")?;
        let mut p = params.clone();
        p.set_input_str(&input_str);
        p.set_otp(&code);
        result = gateway_login(server, cred, &p).await.context("MFA gateway login failed")?;
      }
    }
  }
}

/// Portal getconfig that answers interactive MFA / token challenges, the same
/// way [`gateway_login_mfa`] does for the gateway: in portal mode the RSA/OTP
/// challenge is normally issued by the *portal*, and the later gateway login
/// reuses the portal cookie without re-challenging.
async fn retrieve_config_mfa(
  server: &str,
  cred: &Credential,
  params: &GpParams,
  mfa: &MfaPrompter,
) -> anyhow::Result<PortalConfig> {
  let mut result = retrieve_config(server, cred, params)
    .await
    .context("failed to retrieve the portal configuration")?;
  loop {
    match result {
      PortalConfigResult::Config(config) => return Ok(*config),
      PortalConfigResult::Mfa(message, input_str) => {
        info!("Portal issued an MFA challenge; prompting the user for a code");
        let code = mfa.prompt(message).await.context("MFA challenge cancelled")?;
        let mut p = params.clone();
        p.set_input_str(&input_str);
        p.set_otp(&code);
        result = retrieve_config(server, cred, &p)
          .await
          .context("MFA portal config retrieval failed")?;
      }
    }
  }
}

/// Rebuild the `GpParams` the portal HTTP needs. `is_gateway` selects the
/// prelogin/config endpoint family: the direct-gateway flow (`true`) or the
/// portal flow (`false`).
fn gp_params(
  certificate: Option<String>,
  sslkey: Option<String>,
  key_password: Option<String>,
  ignore_tls_errors: bool,
  os: Option<ClientOs>,
  os_version: Option<String>,
  user_agent: Option<String>,
  is_gateway: bool,
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
  params.set_is_gateway(is_gateway);
  params
}

/// `GpParams` built from a [`ConnectAuthRequest`]'s prelogin/mTLS context.
fn gp_params_from(req: &ConnectAuthRequest, is_gateway: bool) -> GpParams {
  gp_params(
    req.certificate.clone(),
    req.sslkey.clone(),
    req.key_password.clone(),
    req.ignore_tls_errors,
    req.os.clone(),
    req.os_version.clone(),
    req.user_agent.clone(),
    is_gateway,
  )
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
    req.as_gateway,
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
pub async fn build_connect_request(
  req: &ConnectAuthRequest,
  mfa: &MfaPrompter,
  gw: &GatewayPrompter,
) -> anyhow::Result<ConnectRequest> {
  // Prelogin/mTLS + credential resolution use the endpoint family the server
  // is (gateway or portal); `resolve_credential`'s CertOnly prelogin must hit
  // the same one.
  let params = gp_params_from(req, req.as_gateway);
  let cred = resolve_credential(req, &params).await?;

  let (info, cookie) = if req.as_gateway {
    connect_via_gateway(req, &cred, &params, mfa).await?
  } else {
    connect_via_portal(req, &cred, &params, mfa, gw).await?
  };

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
    .with_allow_extend_session(a.allow_extend_session())
    .with_dns_domains(a.dns_domains());

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

/// Direct-gateway login: the server is its own only gateway.
async fn connect_via_gateway(
  req: &ConnectAuthRequest,
  cred: &Credential,
  params: &GpParams,
  mfa: &MfaPrompter,
) -> anyhow::Result<(ConnectInfo, String)> {
  info!("Performing gateway login for {}", req.server);
  let cookie = gateway_login_mfa(&req.server, cred, params, mfa).await?;

  let gateway = Gateway::new(req.server.clone(), req.server.clone());
  let info = ConnectInfo::new(req.server.clone(), gateway.clone(), vec![gateway]);
  Ok((info, cookie))
}

/// Portal login: retrieve the gateway list with the portal credential, pick a
/// gateway, and log into it with the portal cookie (no second interactive
/// auth). `portal_params` must have `is_gateway = false`.
async fn connect_via_portal(
  req: &ConnectAuthRequest,
  cred: &Credential,
  portal_params: &GpParams,
  mfa: &MfaPrompter,
  gw: &GatewayPrompter,
) -> anyhow::Result<(ConnectInfo, String)> {
  info!("Retrieving portal config for {}", req.server);

  // Region drives gateway preference; best-effort (an empty region falls back
  // to the lowest-priority gateway).
  let region = prelogin(&req.server, portal_params)
    .await
    .ok()
    .map(|p| p.region().to_string())
    .unwrap_or_default();

  let mut portal_config = retrieve_config_mfa(&req.server, cred, portal_params, mfa).await?;

  if portal_config.gateways().is_empty() {
    bail!("the portal returned no gateways");
  }
  portal_config.sort_gateways(&region);

  // Clone out of the borrow so the async gateway login below doesn't hold it.
  let preferred = portal_config.find_preferred_gateway(&region).clone();
  let all_gateways: Vec<Gateway> = portal_config.gateways().into_iter().cloned().collect();

  // Always let the user pick (preferred pre-selected), even when the portal
  // offers a single gateway — for now we surface the picker unconditionally so
  // the choice is always visible. A cancelled picker aborts the connect like a
  // cancelled MFA prompt. (The list is non-empty; we bail above otherwise.)
  let selected = {
    info!("Portal offered {} gateway(s); prompting for a choice", all_gateways.len());
    let chosen = gw
      .prompt(req.server.clone(), preferred.clone(), all_gateways.clone())
      .await
      .context("gateway selection cancelled")?;
    all_gateways
      .iter()
      .find(|g| g.server() == chosen)
      .cloned()
      .ok_or_else(|| anyhow::anyhow!("selected gateway is not in the portal's list: {chosen}"))?
  };
  info!("Portal selected gateway: {} ({})", selected.name(), selected.server());

  // The gateway login authenticates with the portal's auth cookie, over the
  // gateway endpoint family.
  let gateway_params = gp_params_from(req, true);
  let gw_cred: Credential = portal_config.auth_cookie().into();
  let cookie = gateway_login_mfa(selected.server(), &gw_cred, &gateway_params, mfa).await?;

  let info = ConnectInfo::new(req.server.clone(), selected, all_gateways);
  Ok((info, cookie))
}
