// `WsEvent` now lives in the shared `gp-protocol` crate; re-exported so
// `gpapi::service::event::*` keeps working across the workspace.
pub use gp_protocol::WsEvent;
