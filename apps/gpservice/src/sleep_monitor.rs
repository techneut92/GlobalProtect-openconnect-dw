//! Resume-from-sleep watcher.
//!
//! After a suspend the tunnel's peer state is dead, but openconnect only finds
//! out via DPD (minutes) and then retries quietly while the UI still claims
//! "Connected". Watch logind's `PrepareForSleep` on the system bus and, on
//! resume, force an immediate teardown-and-reconnect (state → Reconnecting so
//! the UI is honest about it). openconnect's own retry/backoff (up to
//! `reconnect_timeout`) absorbs the seconds Wi-Fi needs to reassociate.
//!
//! Best-effort: if there is no logind (or no system bus — e.g. some dev
//! setups), log and run without the watcher rather than failing the service.

use std::sync::Arc;

use futures::StreamExt;
use log::{info, warn};

use crate::vpn_task::VpnTaskContext;

pub async fn run(ctx: Arc<VpnTaskContext>) {
  if let Err(err) = watch(ctx).await {
    warn!("Sleep monitor unavailable: {}", err);
  }
}

async fn watch(ctx: Arc<VpnTaskContext>) -> anyhow::Result<()> {
  let conn = zbus::Connection::system().await?;
  let proxy = zbus::Proxy::new(
    &conn,
    "org.freedesktop.login1",
    "/org/freedesktop/login1",
    "org.freedesktop.login1.Manager",
  )
  .await?;

  let mut stream = proxy.receive_signal("PrepareForSleep").await?;
  info!("Watching logind PrepareForSleep for suspend/resume");

  while let Some(signal) = stream.next().await {
    let start: bool = match signal.body().deserialize() {
      Ok(start) => start,
      Err(err) => {
        warn!("Failed to parse PrepareForSleep: {}", err);
        continue;
      }
    };

    if start {
      info!("System is going to sleep");
    } else {
      info!("System resumed from sleep");
      ctx.reconnect().await;
    }
  }

  Ok(())
}
