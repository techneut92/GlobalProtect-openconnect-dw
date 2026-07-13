//! Service transport — how the GUI reaches gpservice.
//!
//! One transport for every install (native and Flatpak): the host D-Bus **system**
//! service. gpservice is a dbus-activated root service whose privileged methods
//! are polkit-gated (`io.github.techneut92.gpservice.manage` — an active local
//! user is allowed without a prompt; remote/inactive callers need admin auth), so
//! there's no per-launch pkexec and no loopback socket to secure.

use anyhow::{Context, Result};
use tokio::sync::mpsc;

use crate::dbus_client::{self, DbusHandle};
use gp_protocol::{ConnectRequest, VpnState};

pub struct Transport {
  handle: DbusHandle,
}

impl Transport {
  pub async fn send_connect(&self, request: ConnectRequest) -> Result<()> {
    self.handle.send_connect(serde_json::to_string(&request)?).await
  }

  pub async fn send_disconnect(&self) -> Result<()> {
    self.handle.send_disconnect().await
  }
}

/// Open the transport and return a unified stream of `VpnState` changes.
pub async fn open() -> Result<(Transport, mpsc::Receiver<VpnState>)> {
  let (handle, rx) = dbus_client::open().await.context("connecting to gpservice over D-Bus")?;
  Ok((Transport { handle }, rx))
}
