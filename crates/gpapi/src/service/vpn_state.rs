use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{gateway::Gateway, session::SessionInfo};

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
}

impl ConnectInfo {
  pub fn new(portal: String, gateway: Gateway, gateways: Vec<Gateway>) -> Self {
    Self {
      portal,
      gateway,
      gateways,
    }
  }

  pub fn gateway(&self) -> &Gateway {
    &self.gateway
  }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum VpnState {
  Disconnected,
  Connecting(Box<ConnectInfo>),
  Connected(Box<ConnectedInfo>),
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
