use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// The connect/disconnect/log-level wire types + `WsRequest` moved to the shared
// `gp-protocol` crate; re-exported so `gpapi::service::request::*` keeps working.
// `LaunchGuiRequest` / `UpdateGuiRequest` are not part of the GUI<->service VPN
// protocol (they drive `gpclient launch-gui` and the GUI updater), so they stay.
pub use gp_protocol::{ConnectArgs, ConnectRequest, DisconnectRequest, UpdateLogLevelRequest, WsRequest};

#[derive(Debug, Deserialize, Serialize)]
pub struct LaunchGuiRequest {
  user: String,
  envs: HashMap<String, String>,
}

impl LaunchGuiRequest {
  pub fn new(user: String, envs: HashMap<String, String>) -> Self {
    Self { user, envs }
  }

  pub fn user(&self) -> &str {
    &self.user
  }

  pub fn envs(&self) -> &HashMap<String, String> {
    &self.envs
  }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateGuiRequest {
  pub path: String,
  pub checksum: String,
}
