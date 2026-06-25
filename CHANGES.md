# Modifications

This is a **modified version** of
[GlobalProtect-openconnect](https://github.com/yuezk/GlobalProtect-openconnect)
by Kevin Yue, licensed under **GPL-3.0**. The original copyright notice and the
GPL-3.0 license are retained (see `LICENSE`); this remains a derivative work
distributed under the same license.

Per GPLv3 §5(a), the changes made to the original work are documented below,
with dates. Modifications are © 2026 Dylan Westra and licensed under GPL-3.0.

## 2026-04-29 — Smart-card / PKCS#11 prelogin mTLS

Added smart-card / PKCS#11 client-certificate authentication for the
portal/gateway **prelogin mTLS** (upstream supports only PEM/PKCS#12 *files*):

- **`--certificate pkcs11:<uri>`** — sign the prelogin mTLS on a PKCS#11 token.
  The prelogin uses reqwest + native-tls (which cannot carry a non-extractable
  key), so for `pkcs11:` URIs a rustls `ClientConfig` is built with a
  `cryptoki`-backed signing key and supplied via `use_preconfigured_tls`.
  New file: `crates/gpapi/src/utils/pkcs11.rs`.
- **`--certificate winsign:<thumbprint>`** — sign via Windows `powershell.exe`
  (CNG) against a certificate in the Windows store, for use from WSL without USB
  passthrough. New file: `crates/gpapi/src/utils/winsign.rs`.
- Wired both signers into the prelogin client builder
  (`crates/gpapi/src/gp_params.rs`); registered the modules
  (`crates/gpapi/src/utils/mod.rs`); added a `pkcs11:` guard in `create_identity`
  (`crates/gpapi/src/utils/request.rs`).
- `apps/gpclient/src/connect.rs`: do not pass `winsign:` certificates to
  `openconnect` (it cannot use the scheme); the tunnel uses the auth cookie.
- Dependencies: added `cryptoki`, `rustls`, `rustls-native-certs`; enabled the
  `rustls-tls` feature on `reqwest` (`Cargo.toml`, `crates/gpapi/Cargo.toml`).

No upstream functionality was removed; the file-based `--certificate` path is
unchanged.

## 2026-05-13 — New Tauri GUI (`apps/gpgui`)

Added a new unprivileged graphical client, `apps/gpgui` — a Tauri (HTML/JS +
Rust) front-end that supersedes the previous GUI approach:

- Authentication (prelogin mTLS incl. PKCS#11 + SAML SSO) runs **unprivileged in
  the GUI**, so the embedded auth webview has the user's display; only the tunnel
  runs as root in `gpservice`.
- The GUI depends on the existing `gpapi` auth pipeline and sends a
  `ConnectRequest` to `gpservice` over an encrypted channel; it never holds root.
- Identity/cert management, a smart-card module picker, and a connection manager.

## 2026-05-28 — D-Bus system-service transport (Flatpak)

Added an alternative transport so a sandboxed GUI can reach the root backend:

- `gpservice --dbus` runs as a polkit-gated **D-Bus system service**
  (Connect / Disconnect / Status + a `VpnStateChanged` signal), feeding the same
  `VpnTask` channels as the loopback WebSocket server.
  New file: `apps/gpservice/src/dbus_service.rs`.
- The GUI selects the D-Bus transport inside a Flatpak (`/.flatpak-info`) or via
  `GP_TRANSPORT=dbus`; a `GP_DBUS_SESSION` mode uses the session bus for
  development.
- Added `gpservice --api-key-on-stdin` for the pkexec-launched loopback path.

## 2026-06-11 — Native packaging rework (backend + GUI)

Reworked the native packaging so it builds entirely from source in this fork:

- **Removed the upstream proprietary-GUI download.** `INCLUDE_GUI=1` builds the
  fork's own `gpgui` from source instead.
- One source now produces a **backend** package (`globalprotect-openconnect-dw`:
  `gpservice`/`gpclient`/`gpauth` + D-Bus service + polkit) and a **GUI**
  subpackage (`-gui`, depends on the backend) for deb / rpm / apk / Arch.
- Added PKCS#11 / smart-card runtime dependencies (`openconnect`, `opensc`,
  `pcsc-lite`, `polkit`, `dbus`) across all recipes.
- The package name carries a `-dw` suffix and `Conflicts:`/`provides` so it never
  collides with the upstream `globalprotect-openconnect`.

## 2026-06-19 — Embedded-webview SSO for the native client

- `gpauth` is now built **with** its `webview-auth` feature so the GUI's embedded
  SSO works on native installs (no external browser required); `gpclient` and
  `gpservice` stay lean. The backend gains `webkit2gtk`/`libsecret` deps because
  `gpauth` links webkit.
- The `gpgui` desktop entry sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` to work
  around a broken webkit DMABUF renderer on some systems.

## 2026-06-24 — Rebrand and 1.0.0 release

- Renamed all application identifiers from `com.yuezk.*` to
  `io.github.techneut92.*` (Flatpak app-id, Tauri ids, the D-Bus name / object
  path / interface, polkit actions) and repointed fork URLs to
  `github.com/techneut92/GlobalProtect-openconnect-dw`. Upstream attribution
  (this file, `LICENSE`) and references to upstream issues are intentionally
  preserved.
- Set the fork version to **1.0.0** and added a tag-driven CI release that builds
  all native packages and publishes a GitHub release.
- The Flatpak manifest targets the GNOME 50 runtime.
