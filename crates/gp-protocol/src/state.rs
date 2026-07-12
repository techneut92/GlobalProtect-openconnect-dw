use serde::{Deserialize, Serialize};
use specta::Type;

use crate::gateway::Gateway;
use crate::session::SessionInfo;

#[derive(Debug, Deserialize, Serialize, Type, Clone)]
pub struct ConnectInfo {
  portal: String,
  gateway: Gateway,
  gateways: Vec<Gateway>,
}

#[derive(Debug, Deserialize, Serialize, Type, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedInfo {
  info: Box<ConnectInfo>,
  session_info: Option<SessionInfo>,
  /// Tunnel interface name (e.g. `tun0`), reported by openconnect once the tun
  /// device is up.
  #[serde(skip_serializing_if = "Option::is_none", default)]
  tun_iface: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  ipv4: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  ipv6: Option<String>,
}

impl ConnectedInfo {
  pub fn new(info: ConnectInfo, session_info: Option<SessionInfo>) -> Self {
    Self {
      info: Box::new(info),
      session_info,
      tun_iface: None,
      ipv4: None,
      ipv6: None,
    }
  }

  /// Attach the tunnel facts captured from openconnect.
  pub fn with_tunnel(mut self, tun_iface: Option<String>, ipv4: Option<String>, ipv6: Option<String>) -> Self {
    self.tun_iface = tun_iface;
    self.ipv4 = ipv4;
    self.ipv6 = ipv6;
    self
  }

  pub fn info(&self) -> &ConnectInfo {
    &self.info
  }

  pub fn session_info(&self) -> Option<&SessionInfo> {
    self.session_info.as_ref()
  }

  pub fn tun_iface(&self) -> Option<&str> {
    self.tun_iface.as_deref()
  }

  pub fn ipv4(&self) -> Option<&str> {
    self.ipv4.as_deref()
  }

  pub fn ipv6(&self) -> Option<&str> {
    self.ipv6.as_deref()
  }
}

impl ConnectInfo {
  pub fn new(portal: String, gateway: Gateway, gateways: Vec<Gateway>) -> Self {
    Self {
      portal,
      gateway,
      gateways,
    }
  }

  pub fn portal(&self) -> &str {
    &self.portal
  }

  pub fn gateway(&self) -> &Gateway {
    &self.gateway
  }
}

impl VpnState {
  /// Short human label for the current state (used by the GUI tray/status).
  pub fn label(&self) -> &'static str {
    match self {
      VpnState::Disconnected => "Disconnected",
      VpnState::Connecting(_) => "Connecting…",
      VpnState::Connected(_) => "Connected",
      VpnState::Reconnecting(_) => "Reconnecting…",
      VpnState::Disconnecting => "Disconnecting…",
    }
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum VpnState {
  Disconnected,
  Connecting(Box<ConnectInfo>),
  Connected(Box<ConnectedInfo>),
  /// The tunnel dropped (resume from sleep, network change, DPD failure) and
  /// openconnect is re-establishing it with the existing cookie. Carries the
  /// last `ConnectedInfo` so the UI keeps showing the gateway/session details.
  /// Protocol addition in 1.3.0.
  Reconnecting(Box<ConnectedInfo>),
  Disconnecting,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn connected_state_serializes_session_info() {
    let gateway = Gateway::new("vpn".to_string(), "vpn.example.com".to_string());
    let connect_info = ConnectInfo::new("portal.example.com".to_string(), gateway.clone(), vec![gateway]);
    let session_info = SessionInfo {
      lifetime_secs: Some(43_200),
      allow_extend_session: true,
      ..Default::default()
    };

    let value = serde_json::to_value(VpnState::Connected(Box::new(ConnectedInfo::new(
      connect_info,
      Some(session_info),
    ))))
    .unwrap();

    assert_eq!(value["connected"]["sessionInfo"]["lifetimeSecs"], 43_200);
    assert_eq!(value["connected"]["sessionInfo"]["allowExtendSession"], true);
  }
}
