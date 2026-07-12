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

pub(crate) struct VpnTaskContext {
  vpn_handle: Arc<RwLock<Option<Vpn>>>,
  vpn_state_tx: Arc<watch::Sender<VpnState>>,
  disconnect_rx: RwLock<Option<oneshot::Receiver<()>>>,
  /// Last `ConnectedInfo` sent, kept so the Reconnecting/re-Connected
  /// transitions can carry the same gateway/session details. Std mutex: it is
  /// touched from openconnect's callback thread, never held across awaits.
  connected_info: Arc<std::sync::Mutex<Option<Box<ConnectedInfo>>>>,
}

impl VpnTaskContext {
  pub fn new(vpn_state_tx: watch::Sender<VpnState>) -> Self {
    Self {
      vpn_handle: Default::default(),
      vpn_state_tx: Arc::new(vpn_state_tx),
      disconnect_rx: Default::default(),
      connected_info: Default::default(),
    }
  }

  pub async fn connect(&self, req: ConnectRequest) {
    let vpn_state = self.vpn_state_tx.borrow().clone();
    if !matches!(vpn_state, VpnState::Disconnected) {
      info!("VPN is not disconnected, ignore the request");
      return;
    }

    let vpn_state_tx = self.vpn_state_tx.clone();
    let info = req.info().clone();
    let vpn_handle = Arc::clone(&self.vpn_handle);
    let args = req.args();
    let allow_extend_session = args.allow_extend_session();
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
        vpn_state_tx.send(VpnState::Disconnected).ok();
        return;
      }
    };

    // Each successful internal reconnect (a resume-from-sleep pause, or a DPD
    // failure openconnect recovered from on its own) flips the state back to
    // Connected with the details captured at connect time.
    let connected_info = Arc::clone(&self.connected_info);
    {
      let vpn_state_tx = vpn_state_tx.clone();
      let connected_info = Arc::clone(&connected_info);
      vpn.set_on_reconnected(move || {
        if let Some(info) = connected_info.lock().unwrap().clone() {
          vpn_state_tx.send(VpnState::Connected(info)).ok();
        }
      });
    }

    // Save the VPN handle
    vpn_handle.write().await.replace(vpn);
    let connect_info = Box::new(info.clone());
    vpn_state_tx.send(VpnState::Connecting(connect_info)).ok();

    let (disconnect_tx, disconnect_rx) = oneshot::channel::<()>();
    self.disconnect_rx.write().await.replace(disconnect_rx);

    // Spawn a new thread to process the VPN connection, cannot use tokio::spawn here.
    // Otherwise, it will block the tokio runtime and cannot send the VPN state to the channel
    thread::spawn(move || {
      let vpn_state_tx_clone = vpn_state_tx.clone();
      let connected_info_clone = Arc::clone(&connected_info);

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
        })
      });

      // Notify the VPN is disconnected
      connected_info.lock().unwrap().take();
      vpn_state_tx_clone.send(VpnState::Disconnected).ok();
      // Remove the VPN handle
      vpn_handle.blocking_write().take();

      disconnect_tx.send(()).ok();
    });
  }

  /// Force an immediate teardown-and-reconnect of the live tunnel, reusing the
  /// existing cookie (no re-auth). Called on resume from sleep, where the peer
  /// is dead but DPD would take minutes to notice. No-op unless Connected.
  pub async fn reconnect(&self) {
    let state = self.vpn_state_tx.borrow().clone();
    let VpnState::Connected(info) = state else {
      info!("VPN is not connected, skip reconnect");
      return;
    };
    if let Some(vpn) = self.vpn_handle.read().await.as_ref() {
      info!("Forcing tunnel reconnect");
      self.vpn_state_tx.send(VpnState::Reconnecting(info)).ok();
      vpn.pause();
    }
  }

  pub async fn disconnect(&self) -> bool {
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
