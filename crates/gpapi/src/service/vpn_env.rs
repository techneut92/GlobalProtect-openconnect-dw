// `VpnEnv` now lives in the shared `gp-protocol` crate; re-exported so
// `gpapi::service::vpn_env::*` keeps working across the workspace.
pub use gp_protocol::VpnEnv;
