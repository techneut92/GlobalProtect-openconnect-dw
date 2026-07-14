use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::{sync::Arc, thread};

use gpapi::{
  logger,
  service::{
    request::{ConnectRequest, UpdateLogLevelRequest, WsRequest},
    vpn_state::{ConnectedInfo, VpnState},
  },
  session::{SessionInfo, SessionWarning},
};
use log::{info, warn};
use openconnect::Vpn;
use tokio::sync::{RwLock, mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::gateway_pin::GatewayRoute;

/// How long to wait for the network to come back (polled every second) before
/// (re)attempting a reconnect, and how many times to re-establish a tunnel that
/// died unexpectedly (e.g. openconnect's own reconnect gave up after a resume)
/// before giving up.
const NETWORK_WAIT: Duration = Duration::from_secs(10);
const MAX_RECONNECT_ATTEMPTS: u32 = 10;
/// On resume, how many seconds to keep retrying the gateway re-pin (polled every
/// second) while the NIC comes back before triggering openconnect's reconnect.
const RESUME_NETWORK_WAIT_SECS: u32 = 10;

pub(crate) struct VpnTaskContext {
  vpn_handle: Arc<RwLock<Option<Vpn>>>,
  vpn_state_tx: Arc<watch::Sender<VpnState>>,
  disconnect_rx: RwLock<Option<oneshot::Receiver<()>>>,
  /// Last `ConnectedInfo` sent, kept so the Reconnecting/re-Connected
  /// transitions can carry the same gateway/session details. Std mutex: it is
  /// touched from openconnect's callback thread, never held across awaits.
  connected_info: Arc<std::sync::Mutex<Option<Box<ConnectedInfo>>>>,
  /// The live connection request, kept so a tunnel that dies unexpectedly can be
  /// re-established from scratch (same cookie) without re-auth.
  last_request: Arc<std::sync::Mutex<Option<ConnectRequest>>>,
  /// Set when the user asked to disconnect, so the connection thread knows an
  /// exit was intentional and must NOT be retried.
  user_disconnect: Arc<AtomicBool>,
  /// The gateway's host route (captured over the physical NIC at connect time),
  /// re-asserted on resume so openconnect's in-place reconnect reaches the portal
  /// even after the NIC flap drops the route. Std mutex: touched from openconnect's
  /// callback thread, never held across awaits.
  gateway_route: Arc<std::sync::Mutex<Option<GatewayRoute>>>,
}

impl VpnTaskContext {
  pub fn new(vpn_state_tx: watch::Sender<VpnState>) -> Self {
    Self {
      vpn_handle: Default::default(),
      vpn_state_tx: Arc::new(vpn_state_tx),
      disconnect_rx: Default::default(),
      connected_info: Default::default(),
      last_request: Default::default(),
      user_disconnect: Arc::new(AtomicBool::new(false)),
      gateway_route: Default::default(),
    }
  }

  pub async fn connect(&self, req: ConnectRequest) {
    let vpn_state = self.vpn_state_tx.borrow().clone();
    if !matches!(vpn_state, VpnState::Disconnected) {
      info!("VPN is not disconnected, ignore the request");
      return;
    }

    // Fresh connection: clear the disconnect flag and remember the request so the
    // connection thread can re-establish the tunnel if it dies unexpectedly.
    self.user_disconnect.store(false, Ordering::SeqCst);
    *self.last_request.lock().unwrap() = Some(req.clone());

    let vpn_state_tx = self.vpn_state_tx.clone();
    let info = req.info().clone();
    let allow_extend_session = req.args().allow_extend_session();
    let server = host_of(req.gateway().server());
    // Resolve the gateway once now, while the network is healthy, so the
    // connection thread can capture its physical route without a DNS lookup that
    // would later route through a dead tunnel.
    let gateway_ip = resolve_ip(&server);
    let vpn_handle = Arc::clone(&self.vpn_handle);
    let connected_info = Arc::clone(&self.connected_info);
    let user_disconnect = Arc::clone(&self.user_disconnect);
    let last_request = Arc::clone(&self.last_request);
    let gateway_route = Arc::clone(&self.gateway_route);

    let vpn = match build_vpn(&req, &connected_info, &vpn_state_tx) {
      Some(vpn) => vpn,
      None => {
        vpn_state_tx.send(VpnState::Disconnected).ok();
        return;
      }
    };

    vpn_handle.write().await.replace(vpn);
    let connect_info = Box::new(info.clone());
    vpn_state_tx.send(VpnState::Connecting(connect_info)).ok();

    let (disconnect_tx, disconnect_rx) = oneshot::channel::<()>();
    self.disconnect_rx.write().await.replace(disconnect_rx);

    // Spawn a new thread to process the VPN connection, cannot use tokio::spawn here.
    // Otherwise, it will block the tokio runtime and cannot send the VPN state to the channel
    thread::spawn(move || {
      let mut attempt = 0u32;
      loop {
        // Run openconnect's mainloop (blocks). The callback fires on each
        // (re)connect; openconnect handles its own internal reconnects too.
        {
          let vpn_state_tx = vpn_state_tx.clone();
          let connected_info_clone = Arc::clone(&connected_info);
          let info = info.clone();
          let gateway_route = Arc::clone(&gateway_route);
          let gateway_ip = gateway_ip.clone();
          vpn_handle.blocking_read().as_ref().map(|vpn| {
            vpn.connect(move |vpn_session_info| {
              let tun_iface = vpn_session_info.tun_iface.clone();
              let ipv4 = vpn_session_info.ipv4.clone();
              let ipv6 = vpn_session_info.ipv6.clone();
              let session_info = SessionInfo::from_vpn_session_fields(
                vpn_session_info.lifetime_secs,
                vpn_session_info.user_expires,
                vpn_session_info.lifetime_warning.map(|warning| SessionWarning {
                  prior_secs: warning.prior_secs,
                  message: warning.message,
                }),
                allow_extend_session,
              );
              info!("VPN session info: {}", session_info.log_summary());
              info!("Tunnel: iface={:?} ipv4={:?} ipv6={:?}", tun_iface, ipv4, ipv6);
              let connected_info =
                Box::new(ConnectedInfo::new(info.clone(), Some(session_info)).with_tunnel(tun_iface, ipv4, ipv6));
              // Keep a copy for the Reconnecting/re-Connected transitions.
              *connected_info_clone.lock().unwrap() = Some(connected_info.clone());
              vpn_state_tx.send(VpnState::Connected(connected_info)).ok();
              // Capture the gateway's physical host route so it can be re-pinned on
              // resume: a NIC flap drops that route, and without it openconnect's
              // reconnect sockets fall back to the dead tunnel and hang for the full
              // TCP timeout.
              if let Some(ip) = gateway_ip.as_deref() {
                if let Some(route) = GatewayRoute::capture(ip) {
                  *gateway_route.lock().unwrap() = Some(route);
                }
              }
            })
          });
        }

        // The mainloop exited. If the user asked to disconnect, we're done.
        if user_disconnect.load(Ordering::SeqCst) {
          break;
        }
        attempt += 1;
        if attempt > MAX_RECONNECT_ATTEMPTS {
          warn!("tunnel did not recover after {} attempts; giving up", MAX_RECONNECT_ATTEMPTS);
          break;
        }
        warn!(
          "tunnel exited unexpectedly; re-establishing (attempt {}/{})",
          attempt, MAX_RECONNECT_ATTEMPTS
        );
        // Stay in Reconnecting while we wait for the network and rebuild.
        if let Some(ci) = connected_info.lock().unwrap().clone() {
          vpn_state_tx.send(VpnState::Reconnecting(ci)).ok();
        }
        wait_for_network(&server, NETWORK_WAIT);
        if user_disconnect.load(Ordering::SeqCst) {
          break;
        }
        // Re-establish the tunnel from the stored request (same cookie, no re-auth).
        let req = last_request.lock().unwrap().clone();
        match req.and_then(|req| build_vpn(&req, &connected_info, &vpn_state_tx)) {
          Some(new_vpn) => {
            vpn_handle.blocking_write().replace(new_vpn);
          }
          None => {
            warn!("could not rebuild the tunnel; giving up");
            break;
          }
        }
      }

      // Notify the VPN is disconnected
      connected_info.lock().unwrap().take();
      vpn_state_tx.send(VpnState::Disconnected).ok();
      // Remove the VPN handle
      vpn_handle.blocking_write().take();

      disconnect_tx.send(()).ok();
    });
  }

  /// Force an immediate in-place reconnect of the live tunnel, reusing the
  /// existing session (no re-auth). Called on resume from sleep, where the peer is
  /// dead but DPD would take minutes to notice. No-op unless Connected.
  pub async fn reconnect(&self) {
    let state = self.vpn_state_tx.borrow().clone();
    let VpnState::Connected(info) = state else {
      info!("VPN is not connected, skip reconnect");
      return;
    };
    self.vpn_state_tx.send(VpnState::Reconnecting(info)).ok();

    // Re-pin the gateway's host route to the physical NIC before poking
    // openconnect. On resume the NIC flap drops that route, so openconnect's
    // reconnect/logout sockets fall back to the default route — which still points
    // at the dead tun0 — and hang for the full TCP timeout (~2 min). Re-asserting
    // it lets that control traffic escape over the physical NIC. We deliberately do
    // NOT tear tun0 down: keeping it up means everything except the pinned gateway
    // stays bound to the dead tunnel (fail-closed), so no traffic can leak.
    //
    // Crucially, the resume signal arrives *before* the NIC has carrier, so the
    // re-pin (and any reconnect socket) would fail against a still-dead path. Retry
    // until the re-pin succeeds — a successful `ip route replace` means the NIC is
    // up and the next-hop is on-link — and only then trigger the reconnect, so
    // openconnect connects against a good route instead of hanging on a dead one.
    // Clone the route out (dropping the std MutexGuard) before any await below.
    let route = self.gateway_route.lock().unwrap().clone();
    if let Some(route) = route {
      let mut pinned = false;
      for attempt in 1..=RESUME_NETWORK_WAIT_SECS {
        if route.reassert() {
          pinned = true;
          break;
        }
        info!("waiting for the network to return before reconnect ({attempt}/{RESUME_NETWORK_WAIT_SECS})");
        tokio::time::sleep(Duration::from_secs(1)).await;
      }
      if !pinned {
        warn!("network still down after {RESUME_NETWORK_WAIT_SECS}s; reconnecting anyway");
      }
    } else {
      warn!("no captured gateway route to re-pin; reconnect may be slow");
    }

    if let Some(vpn) = self.vpn_handle.read().await.as_ref() {
      info!("Resume: gateway re-pinned; triggering in-place reconnect (tunnel stays up)");
      vpn.pause();
    }
  }

  /// Return to Disconnected after a failed connect attempt (e.g. the v3
  /// ConnectAuth auth step failed before a tunnel existed).
  pub fn fail_connect(&self) {
    self.vpn_state_tx.send(VpnState::Disconnected).ok();
  }

  pub async fn disconnect(&self) -> bool {
    // Mark this exit intentional so the connection thread does not retry.
    self.user_disconnect.store(true, Ordering::SeqCst);
    if let Some(disconnect_rx) = self.disconnect_rx.write().await.take() {
      info!("Disconnecting VPN...");
      if let Some(vpn) = self.vpn_handle.read().await.as_ref() {
        info!("VPN is connected, start disconnecting...");
        self.vpn_state_tx.send(VpnState::Disconnecting).ok();
        vpn.disconnect()
      }
      // Wait for the VPN to be disconnected
      disconnect_rx.await.ok();
      info!("VPN disconnected");

      true
    } else {
      info!("VPN is not connected, skip disconnect");
      self.vpn_state_tx.send(VpnState::Disconnected).ok();
      false
    }
  }
}

/// Build an openconnect `Vpn` from a request and wire its on-reconnected callback
/// (each internal reconnect flips the state back to Connected with the details
/// captured at connect time). Returns `None` if the VPN can't be created.
fn build_vpn(
  req: &ConnectRequest,
  connected_info: &Arc<std::sync::Mutex<Option<Box<ConnectedInfo>>>>,
  vpn_state_tx: &Arc<watch::Sender<VpnState>>,
) -> Option<Vpn> {
  let args = req.args();
  let vpn = match Vpn::builder(req.gateway().server(), args.cookie())
    .script(args.vpnc_script())
    .user_agent(args.user_agent())
    .os(args.openconnect_os())
    .os_version(args.os_version())
    .client_version(args.client_version())
    .certificate(args.certificate())
    .sslkey(args.sslkey())
    .key_password(args.key_password())
    .hip(args.hip())
    .csd_uid(args.csd_uid())
    .csd_wrapper(args.csd_wrapper())
    .reconnect_timeout(args.reconnect_timeout())
    .mtu(args.mtu())
    .disable_ipv6(args.disable_ipv6())
    .no_dtls(args.no_dtls())
    .local_hostname(args.local_hostname())
    .dpd_interval(args.force_dpd())
    .no_xmlpost(args.no_xmlpost())
    .build()
  {
    Ok(vpn) => vpn,
    Err(err) => {
      warn!("Failed to create VPN: {}", err);
      return None;
    }
  };

  let vpn_state_tx = vpn_state_tx.clone();
  let connected_info = Arc::clone(connected_info);
  vpn.set_on_reconnected(move || {
    if let Some(info) = connected_info.lock().unwrap().clone() {
      vpn_state_tx.send(VpnState::Connected(info)).ok();
    }
  });
  Some(vpn)
}

/// Bare host from a gateway server value (strip scheme/path/port) for a plain TCP
/// reachability probe.
fn host_of(server: &str) -> String {
  server
    .trim_start_matches("https://")
    .trim_start_matches("http://")
    .split('/')
    .next()
    .unwrap_or(server)
    .split(':')
    .next()
    .unwrap_or(server)
    .to_string()
}

/// Resolve a bare host to its first IP address (as a string), for capturing the
/// gateway's physical route. Returns `None` if resolution fails.
fn resolve_ip(host: &str) -> Option<String> {
  use std::net::ToSocketAddrs;
  (host, 443u16)
    .to_socket_addrs()
    .ok()
    .and_then(|mut addrs| addrs.next())
    .map(|addr| addr.ip().to_string())
}

/// Block until `host:443` is reachable or `max` elapses. Runs on the (blocking)
/// connection thread before re-establishing a tunnel that died on a not-yet-ready
/// network after a resume.
fn wait_for_network(host: &str, max: Duration) {
  use std::net::ToSocketAddrs;
  let deadline = std::time::Instant::now() + max;
  loop {
    let reachable = (host, 443u16)
      .to_socket_addrs()
      .ok()
      .and_then(|mut addrs| addrs.next())
      .map(|addr| std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok())
      .unwrap_or(false);
    if reachable {
      info!("network reachable ({host})");
      return;
    }
    if std::time::Instant::now() >= deadline {
      warn!("network still unreachable after {:?}; proceeding anyway", max);
      return;
    }
    std::thread::sleep(Duration::from_secs(1));
  }
}

pub(crate) struct VpnTask {
  ws_req_rx: mpsc::Receiver<WsRequest>,
  ctx: Arc<VpnTaskContext>,
  cancel_token: CancellationToken,
}

impl VpnTask {
  pub fn new(ws_req_rx: mpsc::Receiver<WsRequest>, vpn_state_tx: watch::Sender<VpnState>) -> Self {
    let ctx = Arc::new(VpnTaskContext::new(vpn_state_tx));
    let cancel_token = CancellationToken::new();

    Self {
      ws_req_rx,
      ctx,
      cancel_token,
    }
  }

  pub fn cancel_token(&self) -> CancellationToken {
    self.cancel_token.clone()
  }

  pub async fn start(&mut self, server_cancel_token: CancellationToken) {
    let cancel_token = self.cancel_token.clone();

    tokio::select! {
        _ = self.recv() => {
            info!("VPN task stopped");
        }
        _ = cancel_token.cancelled() => {
            info!("VPN task cancelled");
            self.ctx.disconnect().await;
        }
    }

    server_cancel_token.cancel();
  }

  pub fn context(&self) -> Arc<VpnTaskContext> {
    return Arc::clone(&self.ctx);
  }

  async fn recv(&mut self) {
    while let Some(req) = self.ws_req_rx.recv().await {
      tokio::spawn(process_ws_req(req, self.ctx.clone()));
    }
  }
}

async fn process_ws_req(req: WsRequest, ctx: Arc<VpnTaskContext>) {
  match req {
    WsRequest::Connect(req) => {
      ctx.connect(*req).await;
    }
    WsRequest::Disconnect(_) => {
      ctx.disconnect().await;
    }
    WsRequest::UpdateLogLevel(UpdateLogLevelRequest(level)) => {
      let level = level.parse().unwrap_or_else(|_| log::Level::Info);
      info!("Updating log level to: {}", level);
      if let Err(err) = logger::set_max_level(level) {
        warn!("Failed to update log level: {}", err);
      }
    }
    WsRequest::ConnectAuth(req) => {
      // v3 handoff: the backend authenticates (prelogin + gateway login) and
      // then starts the tunnel via the normal connect path, so state
      // broadcasting is unchanged.
      match crate::auth_flow::build_connect_request(&req).await {
        Ok(request) => ctx.connect(request).await,
        Err(err) => {
          warn!("ConnectAuth failed: {:#}", err);
          // Surface the failure as a return to Disconnected (the GUI shows the
          // error via its own probe/labels; a dedicated error channel is TODO).
          ctx.fail_connect();
        }
      }
    }
    WsRequest::Probe(_) => {
      // Probe returns a ProbeReply to the *requesting* GUI, which needs
      // per-transport response routing (D-Bus method reply / WS event). Handled
      // in the transport layer, not here. TODO: wire ws_server + dbus_service.
      warn!("Probe reached the VPN task; it must be answered by the transport layer");
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn maps_openconnect_session_metadata_to_service_session_info() {
    let info = SessionInfo::from_vpn_session_fields(
      Some(43_200),
      None,
      Some(SessionWarning {
        prior_secs: 1_800,
        message: "Session expires soon".to_string(),
      }),
      true,
    );

    assert_eq!(info.lifetime_secs, Some(43_200));
    assert_eq!(info.expires_in_human.as_deref(), Some("12h"));
    assert_eq!(info.lifetime_warning.unwrap().prior_secs, 1_800);
    assert!(info.allow_extend_session);
  }

  #[test]
  fn direct_request_session_metadata_keeps_extension_disabled() {
    let info = SessionInfo::from_vpn_session_fields(
      Some(43_200),
      None,
      Some(SessionWarning {
        prior_secs: 1_800,
        message: "Session expires soon".to_string(),
      }),
      false,
    );

    assert_eq!(info.lifetime_secs, Some(43_200));
    assert!(!info.allow_extend_session);
  }
}
