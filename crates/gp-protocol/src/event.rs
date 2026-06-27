use serde::{Deserialize, Serialize};

use crate::env::VpnEnv;
use crate::state::VpnState;

/// Events that can be emitted by the service.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum WsEvent {
  VpnEnv(VpnEnv),
  VpnState(VpnState),
  ActiveGui,
  ResumeConnection,
}
