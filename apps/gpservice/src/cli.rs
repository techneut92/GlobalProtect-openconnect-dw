use std::io::Write;
use std::sync::Arc;

use clap::Parser;
use gpapi::clap::InfoLevelVerbosity;
use gpapi::logger;
use gpapi::{
  service::{request::WsRequest, vpn_state::VpnState},
  utils::{redact::Redaction, shutdown_signal},
};
use log::{info, warn};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::vpn_task::VpnTask;

const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", compile_time::date_str!(), ")");

#[derive(Parser)]
#[command(version = VERSION)]
struct Cli {
  /// Accepted for the D-Bus system-service activation file. gpservice is now
  /// D-Bus-only, so this flag is implied.
  #[clap(long)]
  dbus: bool,
  #[command(flatten)]
  verbose: InfoLevelVerbosity,
}

impl Cli {
  async fn run(&mut self) -> anyhow::Result<()> {
    self.init_logger();
    info!("gpservice started: {}", VERSION);
    if !self.dbus {
      info!("gpservice runs as a D-Bus system service only; the --dbus flag is implied");
    }

    // Channel for sending requests to the VPN task.
    let (req_tx, req_rx) = mpsc::channel::<WsRequest>(32);
    // Channel for receiving the VPN state from the VPN task.
    let (vpn_state_tx, vpn_state_rx) = watch::channel(VpnState::Disconnected);
    // Shared slot for an interactive MFA challenge's code, bridged between the
    // D-Bus front-end (submit_mfa) and the connect pipeline (MfaPrompter).
    let mfa_slot: crate::auth_flow::MfaSlot = std::sync::Arc::new(std::sync::Mutex::new(None));

    let mut vpn_task = VpnTask::new(req_rx, vpn_state_tx, mfa_slot.clone());

    // Resume-from-sleep watcher: force an immediate tunnel reconnect after
    // suspend instead of waiting minutes for DPD.
    tokio::spawn(crate::sleep_monitor::run(vpn_task.context()));

    // D-Bus system-service front-end — the only transport. It is system-activated
    // for an external GUI, and single-instance is enforced by owning the bus name.
    // Like the old loopback path, it exits when the controlling GUI goes away.
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(4);
    let vpn_task_cancel_token = vpn_task.cancel_token();
    let server_token = CancellationToken::new();

    let vpn_task_handle = tokio::spawn(async move { vpn_task.start(server_token).await });

    let dbus_shutdown_tx = shutdown_tx.clone();
    let dbus_handle = tokio::spawn(async move {
      if let Err(err) = crate::dbus_service::run(req_tx, vpn_state_rx, dbus_shutdown_tx, mfa_slot).await {
        warn!("D-Bus service error: {}", err);
      }
      let _ = shutdown_tx.send(()).await;
    });

    tokio::select! {
      _ = shutdown_signal() => info!("Shutdown signal received"),
      _ = shutdown_rx.recv() => info!("D-Bus service stopped"),
    }

    vpn_task_cancel_token.cancel();
    let _ = vpn_task_handle.await;
    // Don't join the D-Bus task: its state-broadcast loop only ends when the
    // vpn-state sender drops, and the sleep monitor holds the VpnTask context
    // (and with it the sender) for the life of the process — joining would hang
    // the shutdown forever. The process is exiting; abort it.
    dbus_handle.abort();
    info!("gpservice stopped");
    Ok(())
  }

  fn init_logger(&self) -> Arc<Redaction> {
    let redaction = Arc::new(Redaction::new());
    let redaction_clone = Arc::clone(&redaction);

    let inner_logger = env_logger::builder()
      // Set the log level to the Trace level, the logs will be filtered
      .filter_level(log::LevelFilter::Trace)
      // zbus logs the D-Bus handshake + every method dispatch at INFO, which
      // floods the service journal; cap it at Warn regardless of verbosity.
      .filter_module("zbus", log::LevelFilter::Warn)
      .format(move |buf, record| {
        let timestamp = buf.timestamp();
        writeln!(
          buf,
          "[{} {:<5} {}] {}",
          timestamp,
          record.level(),
          record.module_path().unwrap_or_default(),
          redaction_clone.redact_str(&record.args().to_string())
        )
      })
      .build();

    let level = self.verbose.log_level_filter().to_level().unwrap_or(log::Level::Info);

    logger::init_with_logger(level, inner_logger);

    redaction
  }
}

pub async fn run() {
  let mut cli = Cli::parse();
  if let Err(err) = cli.run().await {
    log::error!("gpservice error: {err}");
  }
}
