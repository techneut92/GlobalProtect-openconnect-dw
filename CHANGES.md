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

## 2026-06-25 — GUI tray redesign, status icons, and startup options

User-facing improvements to the `apps/gpgui` tray client:

- **Redesigned, state-aware tray icon.** Replaced the generic themed `network-*`
  icons with a branded mark rendered to an ARGB pixmap (so it looks identical
  across KDE, COSMIC and GNOME). Two selectable concepts — **Shield** and
  **Signal ring** — each with three colour-coded states (grey = disconnected,
  amber = connecting, green = connected). The **connecting** state is animated by
  swapping frames on a timer (SNI hosts don't play GIFs); `Error` reuses the
  disconnected icon. New assets under `apps/gpgui/icons/tray/`; `tray.rs` decodes
  the PNGs to ARGB via the new `png` dependency.
- **Close-to-tray.** The window's X button hides to the tray and keeps the app
  (tunnel, notifications, tray) running, falling back to a real quit when no tray
  host is present. Left-clicking the tray icon re-opens the window; the
  right-click menu has Open / Disconnect / Quit, and **Quit** tears down the
  tunnel before exiting.
- **Run at system startup** (new; default on). An XDG autostart entry
  (`~/.config/autostart/gpgui.desktop`, launched `--hidden` straight to the tray)
  is reconciled to the preference on every launch and toggled from settings. New
  file: `apps/gpgui/src/autostart.rs`.
- **New "General" settings tab** holding the startup toggle and the tray-icon
  picker; `config.json` gains `tray_icon` and `run_at_startup`.
- **New application icon** — a shield-and-globe brand mark (`icons/icon.svg` plus
  regenerated PNG/ICO).
- **Auto-tiling window managers.** The main window is marked as a dialog
  (`_NET_WM_WINDOW_TYPE_DIALOG`, via a new `gtk` Linux dependency) so X11 tilers
  (Pop Shell on Xorg, i3) float it; `StartupWMClass=gpgui` was added for the
  window↔desktop association. Wayland tilers (COSMIC) cannot be floated by the
  app — there it needs a compositor float rule on app-id `gpgui`.
- **Dropdown popovers** use square corners: webkitgtk doesn't clip
  `overflow:auto` content to a `border-radius`, which made the rounded corners
  render glitchy.

## 2026-06-25 — GUI: connect-from-tray, keyring unlock, window redesign

A second pass over the `apps/gpgui` client:

- **"Connect with ▸" tray submenu** lists the unlocked vault identities; choosing
  one starts a connection via the same path as the window's connect button
  (`tray.rs` gains a `Vault` handle; the connect logic was factored into
  `start_connect`). The tray menu is rebuilt whenever the vault locks/unlocks or
  identities change (`refresh_tray`). When the connecting animation ends, the
  animator forces a few spaced, hash-changing repaints so the SNI host reliably
  drops the spinner frame and shows the final static icon.
- **Keyring unlock (opt-in).** A "Remember unlock" toggle stores the master PIN
  in the desktop secret store via the freedesktop **Secret Service** API
  (GNOME Keyring / KDE KWallet / COSMIC) and auto-unlocks on launch; any
  miss/lock/corruption falls back to the PIN prompt. New file
  `apps/gpgui/src/secrets.rs`; new `keyring` dependency; `config.json` gains
  `remember_unlock`.
- **Forgotten-PIN reset.** The unlock screen offers a guarded reset that deletes
  the encrypted vault (and any stored keyring PIN) and returns to first-run
  setup — saved identities are unrecoverable, so it warns first. New
  `reset_vault` command + `Vault::reset`.
- **Start minimized** option (`config.json` `start_minimized`): open hidden to
  the tray on any launch, not just login autostart.
- **Reverse-DNS app-id.** Enabled Tauri `enableGTKAppId`, so the window's
  Wayland app-id / X11 WM_CLASS is the identifier `io.github.techneut92.gpgui`;
  the GUI desktop entry was renamed to match and all packaging references
  updated. Auto-tiling shells that read a per-app float list (Pop Shell) are now
  registered on startup via `apps/gpgui/src/tiling.rs`; the X11 dialog hint is
  gated to X11 sessions so Wayland keeps the normal window type.
- **Redesigned main window** (460×812): the connect/disconnect control moved to a
  fixed footer action button (Connect → Cancel → Disconnect → Try again) with the
  status line beneath the title; added a **Ko-fi** button (titlebar) and a
  **Support** tab + Ko-fi card in settings, opening the link via a new `open_url`
  command. Errors now surface the real reason (full anyhow context, e.g. "single
  sign-on was cancelled or failed") in red and never leave a stale
  "Authenticating…" line. `.github/FUNDING.yml` adds Ko-fi.

## 2026-06-25 — GUI: About tab, update check, backend install

A `system.rs` module backs an **About** settings tab and two new screens:

- **Update check** against the GitHub Releases API (the GUI and backend ship from
  the same release, so one check covers both): on demand and on startup, with a
  non-blocking in-window banner when a newer version is out. "Update now" runs
  `flatpak update` on Flatpak installs and otherwise opens the release page — the
  native builds have no package repo to upgrade from yet.
- **Backend-missing screen.** If the privileged `gpservice` isn't installed, the
  window shows an install screen with **OS-fitting** guidance (it detects
  Flatpak / rpm-ostree / dnf / apt / pacman / apk / zypper, e.g. the atomic
  Fedora layered-package + reboot flow) and a best-effort **Install** button that
  runs the package-manager command via `pkexec` (through `flatpak-spawn --host`
  when sandboxed), falling back to the printed instructions.
- **GUI↔backend version compatibility.** `gpservice --version` is compared with
  the GUI version; a mismatch is flagged in About and via a banner.
- **About tab** shows the version, OS, install type, backend status, and links
  (the fork repo and the upstream project, GPL-3.0).
- New Tauri commands `system_info` / `check_update` / `run_update` /
  `install_backend`; the `open_url` command is now Flatpak-aware. New `reqwest`
  dependency (already in the tree via `gpapi`).

## 2026-06-25 — Flatpak packaging for the GUI

Made the GUI Flatpak build-ready (`apps/gpgui/packaging/flatpak/`):

- The manifest gains the permissions the new features need: `--share=network`
  (update check), `--talk-name=org.freedesktop.secrets` (keyring),
  `--talk-name=org.freedesktop.Flatpak` (host `flatpak update` / `pkexec` /
  `xdg-open` via `flatpak-spawn --host`), and
  `--filesystem=xdg-config/{autostart,pop-shell}:create` for the host autostart
  entry and Pop Shell float-rule. Real source hashes for the bundled pcsc-lite /
  opensc; `WEBKIT_DISABLE_DMABUF_RENDERER=1` set in the sandbox.
- Code is Flatpak-aware: `autostart.rs` and `tiling.rs` write to the **host**
  `~/.config` (via `system::host_config_dir`), and the autostart entry runs
  `flatpak run …` under Flatpak.
- Added an AppStream `metainfo.xml`, a `flatpak-build.sh` (installs the runtime/
  SDK, vendors the cargo registry to `cargo-sources.json`, runs flatpak-builder),
  and committed the generated `cargo-sources.json`. The backend stays a host
  package — the in-app "backend not installed" screen handles that.

## 2026-06-26 — GUI: backend-install UX, auto-unlock, Flatpak build & polish

Built and verified the Flatpak end-to-end (GNOME 50 SDK) and reworked the
backend-install flow:

- **Backend-install screen** is now a terminal-style card of numbered,
  individually-copyable install steps (per the new design) with the real release
  asset name / arch / version supplied by the backend, a System-type override
  dropdown, "Copy all commands", and a one-click **Install** button that runs the
  real download+install via a single `pkexec` prompt and **waits for the result**
  (honest success/failure — no optimistic "Installing…"). The fork ships via
  GitHub Releases, so dnf/pacman/zypper install straight from the asset URL while
  rpm-ostree/apt/apk download first.
- **Backend presence** is detected over the system **D-Bus** name inside Flatpak
  (the host `gpservice` binary isn't visible in the sandbox).
- **Create-vault** gains an opt-in **"Unlock automatically"** toggle (default off,
  disabled when no keyring) that stores the master PIN in the desktop secret
  store; new `keyring_available` / `set_remember_unlock` commands.
- **About** shows the real **host OS** (`/run/host/os-release` under Flatpak), a
  separate **Flatpak runtime** row, and the baked-in build kind (`GP_BUILD_KIND`).
- **Tray** registers under the unique D-Bus connection name in Flatpak
  (`ksni` `disable_dbus_name`) — owning `StatusNotifierItem-PID-ID` isn't allowed
  in the sandbox.
- **Layout**: vault setup/unlock screens vertically center; Manage-identities and
  Settings are hidden until the vault is open.
- **Flatpak build fixes**: rust-stable extension `//25.08` (GNOME 50's
  freedesktop base), opensc built with `-Wno-error` (GCC 14) and bash-completions
  redirected into `/app`, GUI source path corrected to the repo root.
- All Rust build warnings resolved.
- **Removed the vestigial `gpgui-helper`** — upstream used it to download the
  (formerly closed) `gpgui` binary at runtime; this fork ships `gpgui` as a host
  package, so `GuiLauncher::download_program` and the helper app / launcher /
  `GP_GUI_HELPER_BINARY` constants are dropped. A GUI↔service version mismatch is
  now logged rather than triggering a download.
- **Rebranded the app to "GP Client".** "GlobalProtect" is a Palo Alto Networks
  trademark, so it's no longer used as the product name — only descriptively
  ("connect to GlobalProtect VPN", keywords, the protocol user-agent). Updated
  `productName`, the window title, the `.desktop` `Name`, the AppStream `<name>`,
  and the in-app titlebar / About name.

## 2026-06-27 — Nix flake rebuilt from source; compatibility & packaging polish

- **Nix flake** (`flake.nix`): now builds the entire workspace — including the
  `gpgui` GUI — from the in-tree source. Upstream's flake fetched a release
  source tarball plus a prebuilt (formerly closed) `gpgui` binary; this fork
  ships neither under those names, so `nix build` was broken. Removed the
  obsolete `scripts/update-flake-hashes.sh` and
  `.github/workflows/update-flake-hashes.yaml` (no release-asset hashes left to
  track, and the workflow never fired — `GITHUB_TOKEN` release events don't
  trigger workflows); added `.github/workflows/nix.yaml` to verify `nix build` in
  CI. Build with the git fetcher so the submodules come along:
  `nix build 'git+https://github.com/techneut92/GlobalProtect-openconnect-dw?submodules=1#default'`.
- Dropped the last stale `gpgui-helper` references — a `--replace-fail` line in
  `flake.nix` (which would have failed any Nix build) and a dead path in
  `.dockerignore`.
- **GUI** (`apps/gpgui/src/system.rs`, `apps/gpgui/src/main.rs`): the
  GUI↔backend version-compatibility check now compares only `major.minor` (the
  `z.y` in `vz.y.x`); patch-level differences are compatible and no longer
  warned about.
- **Flatpak**: added `<screenshots>` to the AppStream metainfo for software-store
  listings.
- **Fedora COPR** (`Makefile`, `.github/workflows/copr.yaml`): added a `make srpm`
  target (offline/vendored source RPM) and a CI workflow that submits it to COPR
  on each tag, building the backend and the native `-gui` subpackage for Fedora
  (x86_64 + aarch64). The spec carries no install scriptlets and writes only under
  `/usr`, so it layers cleanly with `rpm-ostree` on atomic Fedora.
- **RPM install test** (`.github/workflows/build.yaml`): a pipeline job installs
  the freshly built RPM in a clean Fedora image (where `dnf` resolves the runtime
  deps) and asserts the rpm-ostree invariants; the GitHub release gates on it.
- **Docs**: README Ko-fi support badge.

### Third-party components

This program is GPL-3.0-or-later, a fork of
[yuezk/GlobalProtect-openconnect](https://github.com/yuezk/GlobalProtect-openconnect)
(GPL-3.0). The Flatpak additionally bundles, built from upstream source by the
manifest (both GPL-compatible):

- **pcsc-lite** — BSD-3-Clause — <https://pcsclite.apdu.fr/>
- **OpenSC** — LGPL-2.1-or-later — <https://github.com/OpenSC/OpenSC>

Rust dependencies are MIT/Apache-2.0 (e.g. `reqwest`, `keyring`, `ksni`, `zbus`,
`png`, `gtk`), all compatible with GPL-3.0.
