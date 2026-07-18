// `ConnectInfo` / `ConnectedInfo` / `VpnState` now live in the shared
// `gp-protocol` crate; re-exported so `gpapi::service::vpn_state::*` keeps
// working across the workspace.
pub use gp_protocol::{ConnectInfo, ConnectedInfo, MfaChallengeInfo, VpnState};
