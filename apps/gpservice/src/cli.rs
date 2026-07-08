use std::sync::Arc;
use std::{collections::HashMap, io::Write};

use anyhow::bail;
use clap::Parser;
use common::constants::GP_SERVICE_LOCK_FILE;
use gpapi::clap::InfoLevelVerbosity;
use gpapi::logger;
use gpapi::{
  process::gui_launcher::GuiLauncher,
  service::{request::WsRequest, vpn_state::VpnState},
  utils::{base64, crypto::generate_key, env_utils, lock_file::LockFile, redact::Redaction, shutdown_signal},
};
use log::{info, warn};
use tokio::sync::{mpsc, watch};

use crate::{vpn_task::VpnTask, ws_server::WsServer};

const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", compile_time::date_str!(), ")");

#[derive(Parser)]
#[command(version = VERSION)]
struct Cli {
  #[clap(long)]
  minimized: bool,
  #[clap(long)]
  env_file: Option<String>,
  /// Read the 32-byte API key as base64 on stdin (shared with the launching GUI),
  /// instead of generating one. Used when an unprivileged GUI launches the
  /// service via pkexec.
  #[clap(long)]
  api_key_on_stdin: bool,
  /// Run as a D-Bus system service instead of the loopback WebSocket server.
  /// This is the transport a Flatpak-sandboxed GUI uses.
  #[clap(long)]
  dbus: bool,
  #[cfg(debug_assertions)]
  #[clap(long)]
  no_gui: bool,

  #[command(flatten)]
  verbose: InfoLevelVerbosity,
}

impl Cli {
  async fn run(&mut self) -> anyhow::Result<()> {
    let redaction = self.init_logger();
    info!("gpservice started: {}", VERSION);

    let pid = std::process::id();
    let lock_file = Arc::new(LockFile::new(GP_SERVICE_LOCK_FILE, pid));

    if lock_file.check_health().await {
      bail!("Another instance of the service is already running");
    }

    let api_key = self.prepare_api_key();

    // Channel for sending requests to the VPN task
    let (ws_req_tx, ws_req_rx) = mpsc::channel::<WsRequest>(32);
    // Channel for receiving the VPN state from the VPN task
    let (vpn_state_tx, vpn_state_rx) = watch::channel(VpnState::Disconnected);

    let mut vpn_task = VpnTask::new(ws_req_rx, vpn_state_tx);

    // D-Bus system-service front-end (Flatpak transport). Feeds the same
    // VpnTask channels as the WS server; no lock file / loopback port.
    if self.dbus {
      let _ = lock_file; // unused on this path
      let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(4);
      let vpn_task_cancel_token = vpn_task.cancel_token();
      let server_token = tokio_util::sync::CancellationToken::new();

      let vpn_task_handle = tokio::spawn(async move { vpn_task.start(server_token).await });
      // The D-Bus service is always externally managed (system-activated for an
      // external GUI), so like the WS path's `exit_on_idle` it must exit when the
      // controlling GUI goes away. Give `run` a shutdown sender so its
      // controller-watchdog can trigger a full shutdown, not just a disconnect.
      let dbus_shutdown_tx = shutdown_tx.clone();
      let dbus_handle = tokio::spawn(async move {
        if let Err(err) = crate::dbus_service::run(ws_req_tx, vpn_state_rx, dbus_shutdown_tx).await {
          warn!("D-Bus service error: {}", err);
        }
        let _ = shutdown_tx.send(()).await;
      });

      tokio::select! {
        _ = shutdown_signal() => info!("Shutdown signal received"),
        _ = shutdown_rx.recv() => info!("D-Bus service stopped"),
      }

      vpn_task_cancel_token.cancel();
      let _ = tokio::join!(vpn_task_handle, dbus_handle);
      info!("gpservice stopped");
      return Ok(());
    }

    // When the key came in on stdin, an external GUI launched us — tie our
    // lifetime to that GUI so we don't linger after it exits.
    let ws_server = WsServer::new(
      api_key.clone(),
      ws_req_tx,
      vpn_state_rx,
      lock_file.clone(),
      redaction,
      self.api_key_on_stdin,
    );

    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(4);
    let shutdown_tx_clone = shutdown_tx.clone();
    let vpn_task_cancel_token = vpn_task.cancel_token();
    let server_token = ws_server.cancel_token();

    #[cfg(unix)]
    {
      let vpn_ctx = vpn_task.context();
      let ws_ctx = ws_server.context();

      tokio::spawn(async move { signals::handle_signals(vpn_ctx, ws_ctx).await });
    }

    let vpn_task_handle = tokio::spawn(async move { vpn_task.start(server_token).await });
    let ws_server_handle = tokio::spawn(async move { ws_server.start(shutdown_tx_clone).await });

    // When the key is supplied on stdin, the service was launched by an already-
    // running (external) GUI, so it must not try to launch its own GUI.
    #[cfg(debug_assertions)]
    let no_gui = self.no_gui || self.api_key_on_stdin;

    #[cfg(not(debug_assertions))]
    let no_gui = self.api_key_on_stdin;

    if no_gui {
      info!("GUI is disabled (externally managed)");
    } else {
      let envs = self.env_file.as_ref().map(env_utils::load_env_vars).transpose()?;

      let minimized = self.minimized;

      tokio::spawn(async move {
        launch_gui(envs, api_key, minimized).await;
        let _ = shutdown_tx.send(()).await;
      });
    }

    tokio::select! {
      _ = shutdown_signal() => {
        info!("Shutdown signal received");
      }
      _ = shutdown_rx.recv() => {
        info!("Shutdown request received, shutting down");
      }
    }

    vpn_task_cancel_token.cancel();
    let _ = tokio::join!(vpn_task_handle, ws_server_handle);

    lock_file.unlock()?;

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

  fn prepare_api_key(&self) -> Vec<u8> {
    if self.api_key_on_stdin {
      return read_api_key_from_stdin();
    }

    #[cfg(debug_assertions)]
    if self.no_gui {
      return gpapi::GP_API_KEY.to_vec();
    }

    generate_key().to_vec()
  }
}

/// Read a base64-encoded 32-byte API key from stdin (one line). Falls back to a
/// random key on any error so the service still starts (the GUI will just fail
/// to authenticate, which is the safe outcome).
fn read_api_key_from_stdin() -> Vec<u8> {
  use std::io::BufRead;

  let mut line = String::new();
  if std::io::stdin().lock().read_line(&mut line).is_err() {
    warn!("Failed to read API key from stdin, generating a random one");
    return generate_key().to_vec();
  }

  match base64::decode_to_vec(line.trim()) {
    Ok(key) if key.len() == 32 => key,
    _ => {
      warn!("Invalid API key on stdin, generating a random one");
      generate_key().to_vec()
    }
  }
}

#[cfg(unix)]
mod signals {
  use std::sync::Arc;

  use log::{info, warn};

  use crate::vpn_task::VpnTaskContext;
  use crate::ws_server::WsServerContext;

  const DISCONNECTED_PID_FILE: &str = "/tmp/gpservice_disconnected.pid";

  pub async fn handle_signals(vpn_ctx: Arc<VpnTaskContext>, ws_ctx: Arc<WsServerContext>) {
    use gpapi::service::event::WsEvent;
    use tokio::signal::unix::{Signal, SignalKind, signal};

    let (mut user_sig1, mut user_sig2) = match || -> anyhow::Result<(Signal, Signal)> {
      let user_sig1 = signal(SignalKind::user_defined1())?;
      let user_sig2 = signal(SignalKind::user_defined2())?;
      Ok((user_sig1, user_sig2))
    }() {
      Ok(signals) => signals,
      Err(err) => {
        warn!("Failed to create signal: {}", err);
        return;
      }
    };

    loop {
      tokio::select! {
        _ = user_sig1.recv() => {
          info!("Received SIGUSR1 signal");
          if vpn_ctx.disconnect().await {
            // Write the PID to a dedicated file to indicate that the VPN task is disconnected via SIGUSR1
            let pid = std::process::id();
            if let Err(err) = tokio::fs::write(DISCONNECTED_PID_FILE, pid.to_string()).await {
              warn!("Failed to write PID to file: {}", err);
            }
          }
        }
        _ = user_sig2.recv() => {
          info!("Received SIGUSR2 signal");
          ws_ctx.send_event(WsEvent::ResumeConnection).await;
        }
      }
    }
  }
}

async fn launch_gui(envs: Option<HashMap<String, String>>, api_key: Vec<u8>, mut minimized: bool) {
  loop {
    let gui_launcher = GuiLauncher::new(env!("CARGO_PKG_VERSION"), &api_key)
      .envs(envs.clone())
      .minimized(minimized);

    match gui_launcher.launch().await {
      Ok(exit_status) => {
        // Exit code 99 means that the GUI needs to be restarted
        if exit_status.code() != Some(99) {
          info!("GUI exited with code {:?}", exit_status.code());
          break;
        }

        info!("GUI exited with code 99, restarting");
        minimized = false;
      }
      Err(err) => {
        warn!("Failed to launch GUI: {}", err);
        break;
      }
    }
  }
}

pub async fn run() {
  let mut cli = Cli::parse();

  if let Err(e) = cli.run().await {
    eprintln!("Error: {}", e);
    std::process::exit(1);
  }
}
