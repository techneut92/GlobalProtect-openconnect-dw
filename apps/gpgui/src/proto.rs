//! Wire types for the gpservice WebSocket protocol.
//!
//! These mirror the externally-tagged enums in
//! `crates/gpapi/src/service/{request,event,vpn_state}.rs`. Nested payloads
//! (ConnectInfo / Gateway / SessionInfo) are kept as `serde_json::Value` so
//! this GUI stays decoupled from the heavy `gpapi`/`openconnect` crates — we
//! only need to *route* on the variant, not model every field yet.
//!
//! Some variants/payloads are deliberately carried but not yet read, so the
//! whole wire protocol stays described here.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// GUI → service. Externally tagged: `{"Disconnect": null}`, `{"Connect": {…}}`.
#[derive(Debug, Serialize)]
pub enum WsRequest {
  /// Full `ConnectRequest` JSON (`{info, args}`); built after auth yields a cookie.
  Connect(Box<Value>),
  /// `DisconnectRequest` is a unit struct on the server → serialize as `null`.
  Disconnect(()),
  UpdateLogLevel(String),
}

/// service → GUI.
#[derive(Debug, Deserialize)]
pub enum WsEvent {
  /// Sent once on connect: current state + discovered vpnc-script/auth paths.
  VpnEnv(Value),
  VpnState(VpnState),
  ActiveGui,
  ResumeConnection,
}

/// Mirrors `service::vpn_state::VpnState` (`#[serde(rename_all = "camelCase")]`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VpnState {
  Disconnected,
  Connecting(Value),
  Connected(Value),
  Disconnecting,
}

impl VpnState {
  pub fn label(&self) -> &'static str {
    match self {
      VpnState::Disconnected => "Disconnected",
      VpnState::Connecting(_) => "Connecting…",
      VpnState::Connected(_) => "Connected",
      VpnState::Disconnecting => "Disconnecting…",
    }
  }
}
