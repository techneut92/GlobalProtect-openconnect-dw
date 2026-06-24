//! D-Bus system-service front-end for gpservice.
//!
//! An alternative to the loopback WebSocket: a Flatpak-sandboxed GUI can't reach
//! the loopback socket or read `/var/run`, but it *can* talk to a host D-Bus
//! system service via `--system-talk-name`. This exposes the same operations
//! (Connect / Disconnect / Status + a VpnStateChanged signal) and forwards them
//! into the identical `VpnTask` channels the WS server uses.
//!
//! Access control is intended to be polkit (an active-local-user rule, like the
//! pkexec path); for now the shipped system-bus policy gates the calls.

use std::collections::HashMap;

use gpapi::service::{
  request::{ConnectRequest, DisconnectRequest, WsRequest},
  vpn_state::VpnState,
};
use log::{info, warn};
use tokio::sync::{mpsc, watch};
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

pub const BUS_NAME: &str = "io.github.techneut92.GPService";
pub const OBJ_PATH: &str = "/io/github/techneut92/GPService";
const POLKIT_ACTION: &str = "io.github.techneut92.gpservice.manage";

/// Check that the D-Bus caller is authorised (polkit action `io.github.techneut92.gpservice.manage`).
/// Skipped on the session bus (dev), where polkit isn't in play.
async fn authorized(header: &Header<'_>) -> bool {
  if std::env::var("GP_DBUS_SESSION").is_ok() {
    return true;
  }
  let Ok(conn) = zbus::Connection::system().await else {
    return false;
  };
  let Ok(authority) = AuthorityProxy::new(&conn).await else {
    return false;
  };
  let Ok(subject) = Subject::new_for_message_header(header) else {
    return false;
  };
  match authority
    .check_authorization(
      &subject,
      POLKIT_ACTION,
      &HashMap::new(),
      CheckAuthorizationFlags::AllowUserInteraction.into(),
      "",
    )
    .await
  {
    Ok(result) => result.is_authorized,
    Err(err) => {
      warn!("polkit check failed: {err}");
      false
    }
  }
}

struct GpService {
  ws_req_tx: mpsc::Sender<WsRequest>,
  vpn_state_rx: watch::Receiver<VpnState>,
}

#[zbus::interface(name = "io.github.techneut92.GPService1")]
impl GpService {
  /// Start a connection. `request` is the JSON `ConnectRequest` ({info, args}).
  async fn connect(&self, #[zbus(header)] header: Header<'_>, request: String) -> zbus::fdo::Result<()> {
    if !authorized(&header).await {
      return Err(zbus::fdo::Error::AccessDenied("not authorised to manage the VPN".into()));
    }
    let req: ConnectRequest = serde_json::from_str(&request)
      .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("invalid ConnectRequest: {e}")))?;
    self
      .ws_req_tx
      .send(WsRequest::Connect(Box::new(req)))
      .await
      .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    Ok(())
  }

  async fn disconnect(&self, #[zbus(header)] header: Header<'_>) -> zbus::fdo::Result<()> {
    if !authorized(&header).await {
      return Err(zbus::fdo::Error::AccessDenied("not authorised to manage the VPN".into()));
    }
    self
      .ws_req_tx
      .send(WsRequest::Disconnect(DisconnectRequest))
      .await
      .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    Ok(())
  }

  /// Current VPN state as JSON (mirrors the `VpnStateChanged` payload).
  async fn status(&self) -> String {
    serde_json::to_string(&*self.vpn_state_rx.borrow()).unwrap_or_else(|_| "null".into())
  }

  #[zbus(signal)]
  async fn vpn_state_changed(emitter: &SignalEmitter<'_>, state: String) -> zbus::Result<()>;
}

/// Claim the bus name, serve the object, and emit `VpnStateChanged` on every
/// state change. Runs until `vpn_state_rx` closes. Uses the **session** bus when
/// `GP_DBUS_SESSION` is set (for development), otherwise the **system** bus.
pub async fn run(ws_req_tx: mpsc::Sender<WsRequest>, mut vpn_state_rx: watch::Receiver<VpnState>) -> anyhow::Result<()> {
  let service = GpService {
    ws_req_tx,
    vpn_state_rx: vpn_state_rx.clone(),
  };

  let session = std::env::var("GP_DBUS_SESSION").is_ok();
  let builder = if session {
    zbus::connection::Builder::session()?
  } else {
    zbus::connection::Builder::system()?
  };

  let conn = builder.name(BUS_NAME)?.serve_at(OBJ_PATH, service)?.build().await?;
  info!(
    "gpservice D-Bus interface ready on the {} bus: {BUS_NAME}",
    if session { "session" } else { "system" }
  );

  // Re-broadcast VPN state changes as a signal.
  let iface_ref = conn.object_server().interface::<_, GpService>(OBJ_PATH).await?;
  loop {
    if vpn_state_rx.changed().await.is_err() {
      break;
    }
    let state = serde_json::to_string(&*vpn_state_rx.borrow()).unwrap_or_default();
    if let Err(e) = GpService::vpn_state_changed(iface_ref.signal_emitter(), state).await {
      warn!("Failed to emit VpnStateChanged: {e}");
    }
  }

  Ok(())
}
