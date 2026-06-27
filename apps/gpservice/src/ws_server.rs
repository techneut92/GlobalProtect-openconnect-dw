use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::extract::ws::Message;
use common::constants::GP_AUTH_BINARY;
use gpapi::{
  service::{event::WsEvent, request::WsRequest, vpn_env::VpnEnv, vpn_state::VpnState},
  utils::{crypto::Crypto, lock_file::LockFile, redact::Redaction},
};
use log::{info, warn};
use openconnect::{find_csd_wrapper, find_vpnc_script};
use serde::de::DeserializeOwned;
use tokio::{
  net::TcpListener,
  sync::{RwLock, mpsc, watch},
};
use tokio_util::sync::CancellationToken;

use crate::{routes, ws_connection::WsConnection};

pub(crate) struct WsServerContext {
  crypto: Arc<Crypto>,
  ws_req_tx: mpsc::Sender<WsRequest>,
  vpn_state_rx: watch::Receiver<VpnState>,
  redaction: Arc<Redaction>,
  connections: RwLock<Vec<Arc<WsConnection>>>,
  /// Live client count, used for the idle-shutdown check.
  active: Arc<AtomicUsize>,
  /// When true (the GUI launched us via `--api-key-on-stdin`), shut the whole
  /// service down shortly after the last client disconnects.
  exit_on_idle: bool,
  /// Cancelling this stops the WS server, which cascades to a full shutdown.
  cancel_token: CancellationToken,
}

impl WsServerContext {
  pub fn new(
    api_key: Vec<u8>,
    ws_req_tx: mpsc::Sender<WsRequest>,
    vpn_state_rx: watch::Receiver<VpnState>,
    redaction: Arc<Redaction>,
    exit_on_idle: bool,
    cancel_token: CancellationToken,
  ) -> Self {
    Self {
      crypto: Arc::new(Crypto::new(api_key)),
      ws_req_tx,
      vpn_state_rx,
      redaction,
      connections: Default::default(),
      active: Arc::new(AtomicUsize::new(0)),
      exit_on_idle,
      cancel_token,
    }
  }

  pub fn decrypt<T: DeserializeOwned>(&self, encrypted: Vec<u8>) -> anyhow::Result<T> {
    self.crypto.decrypt(encrypted)
  }

  pub async fn send_event(&self, event: WsEvent) {
    let connections = self.connections.read().await;

    for conn in connections.iter() {
      let _ = conn.send_event(&event).await;
    }
  }

  pub async fn add_connection(&self) -> (Arc<WsConnection>, mpsc::Receiver<Message>) {
    let (tx, rx) = mpsc::channel::<Message>(32);
    let conn = Arc::new(WsConnection::new(Arc::clone(&self.crypto), tx));

    // Send current VPN state to new client
    info!("Sending current environment to new client");
    let vpn_env = VpnEnv {
      protocol_version: gp_protocol::PROTOCOL_VERSION,
      vpn_state: self.vpn_state_rx.borrow().clone(),
      vpnc_script: find_vpnc_script().map(|s| s.to_owned()),
      csd_wrapper: find_csd_wrapper().map(|s| s.to_owned()),
      auth_executable: GP_AUTH_BINARY.to_owned(),
    };

    if let Err(err) = conn.send_event(&WsEvent::VpnEnv(vpn_env)).await {
      warn!("Failed to send VPN state to new client: {}", err);
    }

    self.connections.write().await.push(Arc::clone(&conn));
    self.active.fetch_add(1, Ordering::SeqCst);

    (conn, rx)
  }

  pub async fn remove_connection(&self, conn: Arc<WsConnection>) {
    self.connections.write().await.retain(|c| !Arc::ptr_eq(c, &conn));
    let remaining = self.active.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);

    // Externally managed (GUI-launched) service: when the launching GUI goes
    // away, no clients remain — shut down after a short grace period so a quick
    // GUI reconnect doesn't kill the service mid-flight.
    if self.exit_on_idle && remaining == 0 {
      let active = Arc::clone(&self.active);
      let token = self.cancel_token.clone();
      tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        if active.load(Ordering::SeqCst) == 0 {
          info!("No GUI client connected; shutting the service down");
          token.cancel();
        }
      });
    }
  }

  fn vpn_state_rx(&self) -> watch::Receiver<VpnState> {
    self.vpn_state_rx.clone()
  }

  pub async fn forward_req(&self, req: WsRequest) -> anyhow::Result<()> {
    if let WsRequest::Connect(ref req) = req {
      self
        .redaction
        .add_values(&[req.gateway().server(), req.args().cookie()])?
    }

    self.ws_req_tx.send(req).await?;

    Ok(())
  }
}

pub(crate) struct WsServer {
  ctx: Arc<WsServerContext>,
  cancel_token: CancellationToken,
  lock_file: Arc<LockFile>,
}

impl WsServer {
  pub fn new(
    api_key: Vec<u8>,
    ws_req_tx: mpsc::Sender<WsRequest>,
    vpn_state_rx: watch::Receiver<VpnState>,
    lock_file: Arc<LockFile>,
    redaction: Arc<Redaction>,
    exit_on_idle: bool,
  ) -> Self {
    let cancel_token = CancellationToken::new();
    let ctx = Arc::new(WsServerContext::new(
      api_key,
      ws_req_tx,
      vpn_state_rx,
      redaction,
      exit_on_idle,
      cancel_token.clone(),
    ));

    Self {
      ctx,
      cancel_token,
      lock_file,
    }
  }

  pub fn context(&self) -> Arc<WsServerContext> {
    Arc::clone(&self.ctx)
  }

  pub fn cancel_token(&self) -> CancellationToken {
    self.cancel_token.clone()
  }

  pub async fn start(&self, shutdown_tx: mpsc::Sender<()>) {
    let listener = match self.start_tcp_server().await {
      Ok(listener) => listener,
      Err(err) => {
        warn!("Failed to start WS server: {}", err);
        let _ = shutdown_tx.send(()).await;
        return;
      }
    };

    tokio::select! {
      _ = watch_vpn_state(self.ctx.vpn_state_rx(), Arc::clone(&self.ctx)) => {
        info!("VPN state watch task completed");
      }
      _ = start_server(listener, self.ctx.clone()) => {
          info!("WS server stopped");
      }
      _ = self.cancel_token.cancelled() => {
        info!("WS server cancelled");
      }
    }

    let _ = shutdown_tx.send(()).await;
  }

  async fn start_tcp_server(&self) -> anyhow::Result<TcpListener> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    let port = local_addr.port();

    info!("WS server listening on port: {}", port);

    self.lock_file.lock(&port.to_string())?;

    Ok(listener)
  }
}

async fn watch_vpn_state(mut vpn_state_rx: watch::Receiver<VpnState>, ctx: Arc<WsServerContext>) {
  while vpn_state_rx.changed().await.is_ok() {
    let vpn_state = vpn_state_rx.borrow().clone();
    ctx.send_event(WsEvent::VpnState(vpn_state)).await;
  }
}

async fn start_server(listener: TcpListener, ctx: Arc<WsServerContext>) -> anyhow::Result<()> {
  let routes = routes::routes(ctx);

  axum::serve(listener, routes).await?;

  Ok(())
}
