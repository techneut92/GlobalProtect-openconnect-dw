//! `gp-protocol` — the versioned wire contract between the GP Client GUI and the
//! `gpservice` backend.
//!
//! This is the single source of truth for the messages exchanged over the
//! loopback WebSocket and the D-Bus system service. Both `gpservice` (via
//! `gpapi`) and `gpgui` depend on this crate, so the two sides can't drift —
//! replacing the old arrangement where `gpapi::service` and `gpgui::proto` were
//! hand-synced copies.
//!
//! The crate is intentionally light (serde types only, no `reqwest`/`openssl`/
//! `cryptoki`) so the GUI can depend on it without pulling the backend's stack.
//!
//! ## Versioning
//! [`PROTOCOL_VERSION`] bumps whenever the wire format changes incompatibly. The
//! two sides exchange it at handshake; a mismatch surfaces an "update GUI/backend"
//! prompt instead of failing cryptically. It is **independent** of the package /
//! release version (GUI 1.5 and backend 1.9 can both speak protocol 1).

/// The wire-protocol version. Bump on any incompatible change to the message
/// types in this crate. Not tied to the app/backend release version.
pub const PROTOCOL_VERSION: u32 = 1;

pub mod os;

pub use os::ClientOs;
