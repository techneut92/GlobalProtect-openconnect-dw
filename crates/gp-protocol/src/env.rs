use serde::{Deserialize, Serialize};

use crate::state::VpnState;

/// Represents the VPN environment configuration.
/// When a client connects, the gpservice sends the current VPN environment
/// to the client so that it can configure itself accordingly.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VpnEnv {
  /// The range of wire-protocol versions the backend can speak (see
  /// [`crate::PROTOCOL_MIN`] / [`crate::PROTOCOL_MAX`]). A backend that predates
  /// the handshake omits these → both default to the baseline `1`.
  #[serde(default = "crate::protocol_baseline")]
  pub protocol_min: u32,
  #[serde(default = "crate::protocol_baseline")]
  pub protocol_max: u32,

  /// The VPN connection state
  pub vpn_state: VpnState,

  /// The default VPN script path
  pub vpnc_script: Option<String>,

  /// The default CSD wrapper script path
  pub csd_wrapper: Option<String>,

  /// The gpauth executable path
  /// Used by the client to launch gpauth for authentication
  pub auth_executable: String,
}
