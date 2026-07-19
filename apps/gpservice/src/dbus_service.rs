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
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use gpapi::service::{
  request::{ConnectRequest, DisconnectRequest, WsRequest},
  vpn_state::VpnState,
};
use log::{info, warn};
use tokio::sync::{mpsc, watch};
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

/// Unique bus name of the GUI that owns the current tunnel (the last caller of
/// `Connect`). Watched so we can drop the tunnel if that process dies without an
/// explicit `Disconnect` — see [`watch_controller`].
type Controller = Arc<Mutex<Option<String>>>;

pub const BUS_NAME: &str = "io.github.techneut92.GPService";
pub const OBJ_PATH: &str = "/io/github/techneut92/GPService";
const POLKIT_ACTION: &str = "io.github.techneut92.gpservice.manage";

/// Check that the D-Bus caller is authorised (polkit action `io.github.techneut92.gpservice.manage`).
/// Skipped on the session bus (dev), where polkit isn't in play.
async fn authorized(header: &Header<'_>) -> bool {
  // Read once: the connect path setenv()s GP_DNS_DOMAINS on the connection
  // thread, and glibc getenv is not safe against a concurrent setenv that
  // grows the environment. Latching at first use keeps this hot path off the
  // environment entirely (the variable is fixed at service start anyway).
  static DEV_SESSION: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
  if *DEV_SESSION.get_or_init(|| std::env::var("GP_DBUS_SESSION").is_ok()) {
    return true;
  }
  // One shared connection for all polkit checks: opening a connection per
  // call was wasteful and re-read bus-address env vars on a hot path (see the
  // GP_DNS_DOMAINS note above). get_or_try_init retries on a failed attempt.
  static POLKIT_CONN: tokio::sync::OnceCell<zbus::Connection> = tokio::sync::OnceCell::const_new();
  let Ok(conn) = POLKIT_CONN.get_or_try_init(zbus::Connection::system).await else {
    return false;
  };
  let Ok(authority) = AuthorityProxy::new(conn).await else {
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
  controller: Controller,
  /// The parked interactive-MFA oneshot the connect pipeline is waiting on;
  /// `submit_mfa` resolves it with the entered code.
  mfa_slot: crate::auth_flow::MfaSlot,
  /// The parked gateway-selection oneshot the portal connect pipeline is
  /// waiting on; `select_gateway` resolves it with the chosen address.
  gw_slot: crate::auth_flow::GatewaySlot,
  /// zbus runs interface methods on its own executor, which is not a tokio
  /// runtime — so `reqwest`-based work (prelogin) must be spawned onto tokio.
  tokio: tokio::runtime::Handle,
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
    // Remember which GUI owns this tunnel so the watchdog can drop it if that
    // process dies without disconnecting (a frontend crash).
    if let Some(sender) = header.sender() {
      *self.controller.lock().unwrap() = Some(sender.to_string());
    }
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
    // Explicit disconnect: stop watching the caller.
    *self.controller.lock().unwrap() = None;
    self
      .ws_req_tx
      .send(WsRequest::Disconnect(DisconnectRequest))
      .await
      .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    Ok(())
  }

  /// v3 handoff: run prelogin and return the required auth as a JSON
  /// `ProbeReply`. Read-only (no tunnel change), so it is not polkit-gated —
  /// it only tells the caller which credential the server wants.
  async fn probe(&self, request: String) -> zbus::fdo::Result<String> {
    let req: gpapi::service::request::ProbeRequest = serde_json::from_str(&request)
      .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("invalid ProbeRequest: {e}")))?;
    // Run the prelogin (reqwest) on the tokio runtime, not the zbus executor —
    // otherwise the HTTP client panics with "no reactor running".
    let reply = self
      .tokio
      .spawn(async move { crate::auth_flow::probe(&req).await })
      .await
      .map_err(|e| zbus::fdo::Error::Failed(format!("probe task failed: {e}")))?;
    serde_json::to_string(&reply).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
  }

  /// v3 handoff: authenticate with a captured credential and start the tunnel.
  /// `request` is the JSON `ConnectAuthRequest`. Progress arrives via
  /// `VpnStateChanged`, exactly like `connect`.
  async fn connect_auth(&self, #[zbus(header)] header: Header<'_>, request: String) -> zbus::fdo::Result<()> {
    if !authorized(&header).await {
      return Err(zbus::fdo::Error::AccessDenied("not authorised to manage the VPN".into()));
    }
    let req: gpapi::service::request::ConnectAuthRequest = serde_json::from_str(&request)
      .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("invalid ConnectAuthRequest: {e}")))?;
    if let Some(sender) = header.sender() {
      *self.controller.lock().unwrap() = Some(sender.to_string());
    }
    self
      .ws_req_tx
      .send(WsRequest::ConnectAuth(Box::new(req)))
      .await
      .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
    Ok(())
  }

  /// Answer an interactive MFA challenge with the one-time code. Resolves the
  /// oneshot the connect pipeline parked while emitting `MfaChallenge`; the
  /// backend then resubmits the gateway login with the code.
  async fn submit_mfa(&self, #[zbus(header)] header: Header<'_>, code: String) -> zbus::fdo::Result<()> {
    if !authorized(&header).await {
      return Err(zbus::fdo::Error::AccessDenied("not authorised to manage the VPN".into()));
    }
    if let Some(tx) = self.mfa_slot.lock().unwrap().take() {
      let _ = tx.send(Some(code));
    }
    Ok(())
  }

  /// Answer a `GatewaySelect` prompt with the chosen gateway's address (the
  /// `server` field of one of the offered gateways). Resolves the oneshot the
  /// portal connect pipeline parked; the backend then logs into that gateway
  /// and continues.
  async fn select_gateway(&self, #[zbus(header)] header: Header<'_>, gateway: String) -> zbus::fdo::Result<()> {
    if !authorized(&header).await {
      return Err(zbus::fdo::Error::AccessDenied("not authorised to manage the VPN".into()));
    }
    if let Some(tx) = self.gw_slot.lock().unwrap().take() {
      let _ = tx.send(Some(gateway));
    }
    Ok(())
  }

  /// Re-request the MFA challenge. GP code challenges come from the user's
  /// authenticator, so there is nothing to re-send server-side yet — this is a
  /// no-op placeholder so the GUI's "resend" affordance has an endpoint.
  async fn resend_mfa(&self, #[zbus(header)] header: Header<'_>) -> zbus::fdo::Result<()> {
    if !authorized(&header).await {
      return Err(zbus::fdo::Error::AccessDenied("not authorised to manage the VPN".into()));
    }
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
pub async fn run(
  ws_req_tx: mpsc::Sender<WsRequest>,
  mut vpn_state_rx: watch::Receiver<VpnState>,
  shutdown_tx: mpsc::Sender<()>,
  mfa_slot: crate::auth_flow::MfaSlot,
  gw_slot: crate::auth_flow::GatewaySlot,
) -> anyhow::Result<()> {
  let controller: Controller = Arc::new(Mutex::new(None));
  let service = GpService {
    ws_req_tx: ws_req_tx.clone(),
    vpn_state_rx: vpn_state_rx.clone(),
    controller: controller.clone(),
    mfa_slot,
    gw_slot,
    tokio: tokio::runtime::Handle::current(),
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

  // Watchdog: this service is long-lived, so if the GUI that started the tunnel
  // vanishes from the bus (crash / kill — not a clean Disconnect) the tunnel
  // would otherwise stay up with no controlling frontend. Drop it — and then exit
  // — when that happens, mirroring the WS path's exit-on-client-loss policy.
  {
    let conn = conn.clone();
    let controller = controller.clone();
    let ws_req_tx = ws_req_tx.clone();
    let shutdown_tx = shutdown_tx.clone();
    tokio::spawn(async move {
      if let Err(e) = watch_controller(conn, controller, ws_req_tx, shutdown_tx).await {
        warn!("D-Bus controller watchdog stopped: {e}");
      }
    });
  }

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

/// Watch the bus for the controlling GUI disappearing. When the tracked unique
/// name loses its owner (the GUI process died), disconnect the tunnel and then
/// exit the service — mirroring the WS path's exit-on-client-loss policy. The
/// D-Bus `.service` re-activates a fresh `gpservice` on the next `Connect`, so
/// each GUI session gets a freshly-initialised PKCS#11 module, which avoids the
/// stale smart-card handle that made reconnects fail ("data not available").
/// An explicit `Disconnect()` call does NOT come through here, so it keeps the
/// service alive for the next `Connect` as before.
async fn watch_controller(
  conn: zbus::Connection,
  controller: Controller,
  ws_req_tx: mpsc::Sender<WsRequest>,
  shutdown_tx: mpsc::Sender<()>,
) -> anyhow::Result<()> {
  let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
  let mut changes = dbus.receive_name_owner_changed().await?;
  while let Some(signal) = changes.next().await {
    let Ok(args) = signal.args() else {
      continue;
    };
    // A name that gained an owner isn't a disconnect; only releases matter.
    if args.new_owner().is_some() {
      continue;
    }
    let gone = args.name().to_string();
    let is_controller = controller.lock().unwrap().as_deref() == Some(gone.as_str());
    if is_controller {
      info!("Controlling GUI {gone} left the bus; disconnecting the tunnel");
      let _ = ws_req_tx.send(WsRequest::Disconnect(DisconnectRequest)).await;
      *controller.lock().unwrap() = None;
      // Brief grace: let the disconnect's openconnect/vpnc teardown run, and let
      // a fast GUI relaunch re-claim (a new Connect sets `controller` again)
      // before we exit — same shape as the WS wrapper's post-disconnect grace.
      tokio::time::sleep(std::time::Duration::from_secs(2)).await;
      if controller.lock().unwrap().is_some() {
        continue; // a new GUI reconnected within the grace period
      }
      info!("No controlling GUI; shutting the service down");
      let _ = shutdown_tx.send(()).await;
      break;
    }
  }
  Ok(())
}
