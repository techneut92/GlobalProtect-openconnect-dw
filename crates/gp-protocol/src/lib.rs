// gp-protocol — GP Client wire protocol (GUI ↔ gpservice)
// Copyright (C) 2026 Dylan Westra (techneut92)
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version. This program is distributed WITHOUT ANY WARRANTY. See the LICENSE
// file in this directory, or <https://www.gnu.org/licenses/>.
//
// A fork of yuezk/GlobalProtect-openconnect (GPL-3.0).

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

pub mod gateway;
pub mod os;
pub mod session;
pub mod state;

pub use gateway::{Gateway, PriorityRule};
pub use os::ClientOs;
pub use session::{format_duration_secs, SessionInfo, SessionWarning};
pub use state::{ConnectInfo, ConnectedInfo, VpnState};
