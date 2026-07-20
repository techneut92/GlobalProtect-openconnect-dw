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
- **COPR publish gated** (`.github/workflows/build.yaml`): the COPR upload is a
  `copr-publish` job that `needs` the RPM install test, so a package that fails
  the smoke test is never published. The standalone `copr.yaml` is now
  manual-only (`workflow_dispatch`) to avoid a second, ungated publish on tags.
- **COPR Enterprise Linux 10**: the COPR project now also builds for EPEL 10
  (RHEL 10, AlmaLinux 10, Rocky 10, CentOS Stream 10) on x86_64 + aarch64.
- **Distro Rust constraint (documented):** the dependency tree requires
  rustc ≥ 1.88 (`time` 1.88, `zbus` 1.87, `icu` 1.86), and the workspace is
  edition 2024 (≥ 1.85). Source-build packaging therefore only works on distros
  shipping a recent Rust — Fedora, openSUSE Tumbleweed, EL 10. Debian ≤ 13,
  Ubuntu LTS, and EL 9 ship older Rust and can't build from source; those users
  use the Flatpak or the prebuilt `.deb`/`.rpm`. (This is why the openSUSE OBS /
  Debian-Ubuntu PPA roadmap items are constrained.)
- **Docs**: README Ko-fi support badge.
- **Ubuntu 26.04 apt repo (OBS)**: the backend + native `-gui` are built on the
  openSUSE Build Service for Ubuntu 26.04 (Rust 1.93 ≥ the 1.88 floor) and served
  as a signed apt repo. The deb `Build-Depends` gained `rust-1.89-all | rust-all`
  so the build resolves on Ubuntu's `rust-all`. Older Debian/Ubuntu keep the
  prebuilt `.deb` (runs on Debian 12+/Ubuntu 22.04+, glibc ≥ 2.34) + the Flatpak.
- **Deb install test** (`.github/workflows/build.yaml`): installs the freshly
  built `.deb` in a clean Ubuntu image and gates the release, mirroring the rpm
  install test.

## 2026-06-27 — Shared `gp-protocol` wire-protocol crate + version negotiation

Extracted the GUI↔`gpservice` wire contract into its own crate so the two sides
can't drift, and gave it a negotiated version range — groundwork for the planned
independent backend/GUI versioning and repo split (see `docs/split-plan.md`,
Phase 1).

- **New `crates/gp-protocol`** (© 2026 Dylan Westra, GPL-3.0): the single source
  of truth for the messages exchanged over the loopback WebSocket and the D-Bus
  service. All wire types moved here out of `gpapi` — `ClientOs`, `Gateway`,
  `SessionInfo`/`SessionWarning` (+ the time-formatting helpers), `ConnectInfo`/
  `ConnectedInfo`/`VpnState`, the `ConnectArgs`/`ConnectRequest`/`WsRequest`
  cluster, `WsEvent`, `VpnEnv`. `gpapi::service` now re-exports them so call
  sites are unchanged. The crate is light (serde only — no `reqwest`/`openssl`/
  `cryptoki`) so the GUI can depend on it without the backend's stack.
- **Deleted `apps/gpgui/src/proto.rs`** — the GUI's hand-synced, `Value`-payload
  mirror of the protocol. `gpgui` depends on `gp-protocol` directly and is now
  typed end-to-end (`send_connect` takes a `ConnectRequest`; `parse_conn_details`
  reads a typed `ConnectedInfo`). The structural source of GUI↔backend drift is
  gone.
- **Protocol version negotiation** (`gpservice` ↔ `gpgui`): `VpnEnv` advertises
  the backend's `PROTOCOL_MIN..=PROTOCOL_MAX` range; the GUI negotiates the
  highest version both support at connect and refuses only when the ranges don't
  overlap — naming which side is too old. Missing fields default to baseline `1`,
  so a backend that predates the handshake stays compatible. Native/loopback
  transport; the Flatpak/D-Bus path keeps the package-version compatibility check.
- **Wire-format CI guard** (`crates/gp-protocol/tests/wire_format.rs` + a
  `wire-format-guard` job in `.github/workflows/build.yaml`): a snapshot test
  serializes every top-level protocol message to JSON and fails the build if the
  wire format changes without a deliberate snapshot regen — so the protocol can't
  drift unnoticed, and a real change forces a `PROTOCOL_MAX` decision.
- **Quieter service journal**: `gpservice` caps the `zbus` log target at `warn`
  (`apps/gpservice/src/cli.rs`). zbus logs the D-Bus handshake and every method
  dispatch at INFO, which flooded the journal; the service's own logs are
  unchanged.

## 2026-06-27 — Release-pipeline fixes (1.1.0)

- **Flatpak**: bundle `pcsc-lite` from its **git tag + meson** rather than the
  apdu.fr tarball. apdu.fr keeps only the latest two releases, so a pinned
  tarball 404s on the next upstream release (it broke the 1.1.0 flatpak build);
  git tags are stable and pcsc-lite has used meson since 2.x. (Also fixed the
  `COPYING` license-copy path for meson's out-of-tree build.)
- **CI**: `copr-publish` now gates on the **full build set** including the GUI
  flatpak (matching `gh-release`). A release is atomic — the backend can't
  publish to COPR if any build phase failed.

## 2026-06-27 — GUI: update badge, startup update check, drop version-mismatch warning

- **Update-available badge** (`apps/gpgui/ui/`): a download-icon badge
  (`update-badge.png`) shows on the settings gear (`index.html`) and the About
  nav item (`settings.html`) when `check_update` finds a newer release. The check
  runs on startup via `refreshBanners()`.
- **Removed the version-mismatch warning**: the GUI no longer warns when its
  `major.minor` differs from the backend's package version (the old heuristic).
  Real compatibility is enforced by the `gp-protocol` handshake at connect, so a
  version difference is harmless. The About header now shows the backend version
  beside the app version, and the "Update backend" button is gated on an available
  update rather than a mismatch.

## 2026-06-28 — Webkit-free backend: SAML webview moved into the GUI

Phase 2 of the backend/GUI split (`docs/split-plan.md`). The GUI already owned the
auth flow (prelogin + SSO + building the `ConnectRequest`); it just delegated the
SAML webview to a spawned `gpauth` subprocess. That subprocess was the only thing
pulling webkit into the backend, so it's moved in-process.

- **A — GUI runs SSO in-process** (`apps/gpgui`): depends on the `auth` crate
  (`webview-auth` + `browser-auth`) and runs `WebviewAuthenticator` /
  `BrowserAuthenticator` directly with its own Tauri `AppHandle` (threaded
  `setup` → `vpn::run` → `connect` → `build_connect_request`) instead of spawning
  `gpauth`. `SamlAuthData → Credential` via `Credential::try_from`.
- **B — backend is webkit-free**: `gpauth` is now a browser-only SAML helper
  (dropped the `webview-auth` feature, `tauri`/`tauri-build` deps, the embedded
  webview path and `build.rs`). `gpservice`/`gpclient`/`gpauth` have **0
  webkit/tauri deps**; the backend `.deb`/`.rpm` no longer require `libwebkit2gtk`
  (the `-gui` package keeps it). Side benefit: the in-process webview shares one
  cookie store, so SSO is remembered across reconnects within a GUI session.

## 2026-07-07 — GUI: single-instance relaunch fix

Added `tauri-plugin-single-instance` (registered first) to `apps/gpgui`. On
Linux, Tauri/WebKitGTK registers a GTK application under a unique app id, so
relaunching while the app was still running (closed to the tray) forwarded the
activation into the live instance and re-ran Tauri's `setup()` there — panicking
with *"a webview with label `main` already exists"* and taking down the running
app. The plugin's callback now fires in the running instance and simply
shows/unminimizes/focuses the existing `main` window; the second process exits.

## 2026-07-07 — Backend: re-init PKCS#11 before loading a client cert

`gpservice` is long-lived and the tunnel runs in-process (no fork per connect),
but GnuTLS's PKCS#11 token cache is process-global. After a smart-card re-seat, a
`pcscd` cycle, or a suspend/resume, that cache went stale and
`gnutls_pkcs11_obj_import_url()` returned `GNUTLS_E_REQUESTED_DATA_NOT_AVAILABLE`
("data not available") even though the cert was physically present — the only
workaround was restarting the service. `crates/openconnect/src/ffi/vpn.c` now
calls `gnutls_pkcs11_reinit()` before loading a `pkcs11:` client cert so a
re-seated token's certs are found automatically. File certs are unaffected.

## 2026-07-07 — GUI: sandbox-safe single-instance; backend: drop tunnel on frontend loss

Two related robustness fixes for the tray-relaunch crash.

- **GUI single-instance without D-Bus** (`apps/gpgui`): `tauri-plugin-single-instance`
  detects the second instance over a D-Bus session name, which doesn't work in the
  Flatpak sandbox — so GTK's GApplication forwarded the relaunch into the primary,
  re-ran setup, and panicked (`a webview with label `main` already exists`). Replaced
  with an abstract-namespace Unix socket claimed at the top of `main()`, before any
  GTK/Tauri init: the second instance signals the primary to reveal its window and
  exits pre-init. Shared across Flatpak instances via the manifest's `--share=network`.
  Dropped the plugin dependency. Also: the main-window "update available" banner now
  opens the About page's unified Update-all flow instead of a frontend-only update.
- **Tunnel teardown on frontend loss** (`apps/gpservice`): both transports forward
  into the same `VpnTask`, so teardown is one shared action (send `Disconnect`) and
  each transport detects its client vanishing — the WebSocket dropping (native; a
  short grace period ignores a reconnecting relaunch, and close-to-tray keeps the
  socket open) and, for the persistent D-Bus service (Flatpak), the `Connect` caller's
  unique bus name losing its owner (`NameOwnerChanged`). No `tun0` is left up without a
  controlling frontend.

## 2026-07-07 — GUI: install the backend at the latest version during "Update all"

The one-click backend installer derived its download URL from `GUI_VERSION`
(`env!("CARGO_PKG_VERSION")`, the running binary's compile-time version) rather
than the target release. During "Update all" the flatpak GUI update only takes
effect after restart, so `GUI_VERSION` stayed at the old version for the whole
run — the backend was (re)installed at the old version, and on rpm-ostree
re-layering the same version is a no-op, leaving the backend behind until a
second Update-all. `install_backend` / `backend_install_script` now take an
explicit target version; the updater passes the latest release (first-run install
still defaults to `GUI_VERSION`, a matched pair). Separately, `check_update` no
longer treats an unreadable installed-backend version as "up to date" — it offers
the update instead of silently skipping it.

## 2026-07-07 — GUI: "Start minimized" governs the autostart launch

The XDG autostart entry always ran the GUI with `--hidden`, and startup treats
`--hidden || start_minimized` as "start hidden" — so the login launch was
unconditionally minimized to the tray, and the "Start minimized" toggle only
affected manual launches. `autostart::set` now takes the `minimized` preference
and appends `--hidden` to the entry only when it is set (both callers — the
startup sync and `save_settings` — pass it), so the toggle governs the login
launch too.

## 2026-07-08 — Backend: D-Bus service exits with the GUI; GUI: no webview zoom

- **`gpservice` D-Bus lifecycle** (`apps/gpservice`): both transports feed the same
  `VpnTask`, but the exit-on-client-loss policy lived only in the WS wrapper
  (`exit_on_idle`). The D-Bus wrapper (added for the tun0 teardown) mirrored only
  the *disconnect* half, so a Flatpak `gpservice` stayed alive after the GUI died
  and kept its opensc/PKCS#11 module loaded — after the first TLS client-auth the
  cached smart-card handle went stale and every reconnect failed with "The
  requested data were not available" until a service restart (`gnutls_pkcs11_reinit`
  alone didn't clear it). `watch_controller` now disconnects **and** signals a full
  shutdown (threaded `shutdown_tx` through `dbus_service::run`); D-Bus re-activates a
  fresh process on the next `Connect`, so each GUI session re-initialises PKCS#11.
  The explicit `Disconnect()` method stays non-fatal.
- **GUI zoom disabled** (`apps/gpgui`): the fixed-size, non-resizable windows now
  swallow Ctrl+wheel / pinch / Ctrl+±/0 (`ui/no-zoom.js`, loaded by all three
  pages) so accidental zoom can't distort the layout.

## 2026-07-11 — GUI: reliably focus the window when revealed from the tray

- **Window activation** (`apps/gpgui`): revealing the main window from the tray
  icon, the tray menu's "Open GP Client", or a second launch called `show()` +
  `unminimize()` + `set_focus()`, but on Wayland compositors (COSMIC, and Mutter
  under some settings) focus-stealing prevention silently drops a `set_focus()`
  that arrives without a valid activation token — the window appeared behind
  other windows and unfocused. The reveal path is now a single `reveal_window`
  helper (`tray.rs`, used by both the tray and `main.rs`'s single-instance
  `serve` callback) that briefly toggles `set_always_on_top(true/false)` around
  the focus request, forcing the compositor to raise **and** activate it.

## 2026-07-12 — GUI: reveal the window on the GTK main thread (crash fix)

- **Intermittent SIGSEGV on window reveal** (`apps/gpgui`): `tray::reveal_window`
  runs Tauri window calls (`show`/`unminimize`/`set_always_on_top`/`set_focus`),
  which on Linux are GTK calls executed on the *calling* thread — and both
  callers are worker threads (the ksni tray service thread and the
  single-instance listener thread). GTK is single-threaded; a core dump showed
  the crash in `gtk_window_realize` → `g_source_set_name_full` under
  `start_thread`, typically on the window's **first** show after starting hidden
  in the tray. The helper now marshals its body onto the main thread via
  `AppHandle::run_on_main_thread`, making it safe from any caller.

## 2026-07-12 — Ported upstream 2.6.x improvements

Selectively ported from upstream (`yuezk/GlobalProtect-openconnect` 2.6.0–2.6.4);
adapted to this fork's layout rather than merged:

- **`host-id` prelogin/login param** (`crates/gpapi/src/gp_params.rs`, upstream
  f98e033): `GpParams` now carries a stable per-machine UUID (same derivation as
  the HIP report) sent as `host-id`. PAN-OS binds the portal's
  authentication-override cookie to this value; without it the portal returns an
  empty cookie and gateway SAML fails with `saml-auth-status=-1`. Our
  `prelogin.rs` already listed `host-id` in its required-params filter but the
  value was never set.
- **Portal-cookie cache** (`crates/gpapi/src/cookie_store.rs` new,
  `apps/gpclient/src/connect.rs`, upstream 7a571f9): `gpclient connect` saves the
  portal auth cookie (0600, atomic write, versioned, server-bound) and on the
  next run logs in straight to the last gateway, skipping prelogin + SAML. New
  flags `--cookie-file` and `--no-cookie-cache`; the cache is cleared when the
  gateway rejects it.
- **External-browser auth robustness** (upstream ecc9c9f, a7a5d9b):
  `Browser::Auto` ("`--browser`" with no value) prefers Chrome/Firefox and falls
  back to the system default (`crates/auth/src/browser/browser_auth.rs`); the
  GUI's browser SSO uses "auto" too. New
  `crates/gpapi/src/process/desktop_session_env.rs` recovers the user's session
  env (DISPLAY, DBUS_SESSION_BUS_ADDRESS, XDG_*) in `into_non_root`, so browser
  auth launched from a root context reaches the graphical session. The auth
  callback server answers HEAD probes without consuming the single-use URL
  (`crates/auth/src/browser/auth_server.rs`).
- **Microsoft Defender HIP entry** (`apps/gpclient/src/hip.rs`,
  `templates/hip_report.xml`, upstream 67de920): if `mdatp health` reports
  Defender on Linux, the HIP report's anti-malware section includes it (version,
  definitions, real-time protection).

Evaluated and **not** ported (bug absent or diverged by design): OTP-on-MFA-retry
duplicate-`passwd` fix (our `HashMap` param builder can't duplicate keys), direct
gateway browser-auth mode gating (our browser mode is response-driven), the
2.6.x packaging/profile restructure, and NixOS/macOS/CI changes. The vendored
openconnect pin was reviewed against upstream master (23 commits): no
GlobalProtect, GnuTLS/PKCS#11, or security changes — pin unchanged.
## 2026-07-12 — SSO webview: build and raise the auth window on the GTK main thread

- **Intermittent SIGSEGV during SSO** (`crates/auth/src/webview/webview_auth.rs`,
  `crates/gpapi/src/utils/window.rs`): this fork runs SAML SSO **in-process** in
  the GUI (upstream's separate `gpauth` binary builds its auth window on its own
  main thread), so `WebviewAuthenticator::authenticate` executes on a Tauri
  async worker. Two raw-GTK-on-the-calling-thread sites resulted: the
  `WebviewWindow` builder (GTK window creation), and `WindowExt::raise`'s
  Wayland branch (`gtk_win.hide()`/`show_all()`), fired by the auth window's
  10-second raise timer when sign-in needs interaction. GTK is single-threaded;
  a captured core showed the worker mid-realize-cascade while the main loop
  crashed in a recursive widget traversal (GitHub #24's original signature —
  the crash was daily because a valid SSO cookie finishes silently in under
  10 s and never arms the raise path). Both sites are now marshalled onto the
  GTK main thread via `run_on_main_thread` (a tokio oneshot hands the built
  window back to the async flow) — the same treatment the tray reveal got in
  1.2.11. Fixes #24.
## 2026-07-12 — CI: automated OBS (Ubuntu apt repo) release bump

- New `scripts/obs-publish.sh` + tag-gated `obs-publish` CI job
  (`.github/workflows/build.yaml`): after the GitHub release is published, CI
  checks out the `home:Techneut92:gp-client/globalprotect-openconnect-dw` OBS
  package, points `_service` at the new `.offline.tar.gz` release asset, sets
  the `.dsc` Version, prepends a `debian.changelog` entry generated from
  `changelog.md`, and `osc commit`s — the Ubuntu build then runs on the OBS
  servers as before. Authenticates via the `OBS_USERNAME`/`OBS_PASSWORD`
  repository secrets.
## 2026-07-12 — Suspend/resume: immediate reconnect + honest "Reconnecting" state

After a suspend the tunnel's peer state is dead, but openconnect only noticed
via DPD (minutes) and then retried silently for up to `reconnect_timeout`
(300 s) — all while the client still reported **Connected** and traffic hung.

- **`gpservice` sleep monitor** (`apps/gpservice/src/sleep_monitor.rs`): watches
  logind's `PrepareForSleep` on the system bus (zbus, both transports). On
  resume, if Connected, it forces an immediate teardown-and-reconnect.
- **openconnect FFI** (`crates/openconnect`): new `vpn_pause()` writes
  `OC_CMD_PAUSE` to the command pipe — the mainloop returns 0 and the existing
  `vpn_connect` loop re-enters it, reconnecting with the same cookie (no
  re-auth; the CLI's SIGUSR2 mechanism). Registered
  `openconnect_set_reconnected_handler` and exposed it as a repeatable
  `Vpn::set_on_reconnected` callback (also fires for DPD-triggered internal
  reconnects). Fixed the crate's unit tests for the tunnel fields added to
  `VpnSessionInfoRaw` earlier.
- **Protocol** (`crates/gp-protocol`): new `VpnState::Reconnecting(ConnectedInfo)`
  variant — a **breaking protocol addition**, so the next release is **1.3.0**
  (GUI and backend must move together). `PROTOCOL_MIN`/`PROTOCOL_MAX` bumped to
  **2** (MIN too: there is no speak-down machinery, so claiming v1 support would
  hand old GUIs an unparseable state — the hard break surfaces the designed
  "update GUI/backend" prompt instead). Wire snapshot regenerated with the new
  `Reconnecting` sample.
- **State plumbing** (`apps/gpservice/src/vpn_task.rs`): the last
  `ConnectedInfo` is kept; `reconnect()` emits `Reconnecting(info)` and pauses;
  the reconnected callback re-emits `Connected(info)`. A failed reconnect falls
  through the existing mainloop-exit path to `Disconnected`.
- **GUI** (`apps/gpgui`): new `Status::Reconnecting` — amber animated tray icon,
  "Reconnecting…" labels, webview keeps the connected-details view and the
  elapsed clock, Disconnect stays available; state `kind` 4 in the webview
  payload.

## 2026-07-12 — gp-protocol extracted to its own project

- Removed `crates/gp-protocol` from this tree. The GUI↔backend wire protocol
  now lives at <https://github.com/techneut92/gp-protocol> — an independent
  work © 2026 Dylan Westra, licensed MIT OR Apache-2.0, re-authored from the
  wire shape (field names/value vocabularies are protocol facts; the
  serialized form is pinned by its `wire_format` snapshot test, carried over
  and passing unchanged). This GPL work consumes it as a tag-pinned
  dependency (`gpapi`, `gpservice`, `gpgui` Cargo.toml → workspace
  dependency), which is license-compatible (MIT/Apache-2.0 → GPL-3.0).
- CI: the `wire-format-guard` job moved to the gp-protocol repository; the
  Flatpak `cargo-sources.json` was regenerated to carry the git source.

## 2026-07-12 — Server-side authentication handoff (wire-protocol v3)

Moved GlobalProtect authentication into `gpservice` so an unprivileged,
GPL-free GUI (the forthcoming GP Client) can connect without linking `gpapi` or
`auth`. Uses `gp-protocol` 1.1.0 (protocol v3, additive — `MIN` stays 2):

- **`crates/gpapi/src/service/request.rs`** re-exports the new
  `ProbeRequest`/`ProbeReply`/`AuthCredential`/`ConnectAuthRequest` handoff types.
- **`apps/gpservice/src/auth_flow.rs`** (new): runs prelogin (including the
  PKCS#11 smart-card mTLS), builds the gateway credential from the GUI-supplied
  result (password / SAML cookie / cert), performs the gateway login, and hands
  a `ConnectRequest` to the existing tunnel path. Mirrors the client-side flow
  the fork's `apps/gpgui` used, moved into the service.
- **D-Bus transport** (`apps/gpservice/src/dbus_service.rs`): new `probe`
  (read-only, not polkit-gated) and `connect_auth` (polkit-gated, like
  `connect`) methods. The probe runs its HTTP work on the tokio runtime — zbus
  dispatches interface methods on its own executor, where `reqwest` would panic.
- **`apps/gpservice/src/vpn_task.rs`**: `WsRequest::ConnectAuth` reuses the
  connect path (state broadcasting unchanged); `Probe` is answered by the
  transport layer.

The existing GUI's connect path (a client-built `ConnectRequest`) is untouched
and unaffected. The D-Bus path (the sandboxed-GUI transport) is complete; the
loopback WS transport (whose `Probe` routing was still a follow-up here) was
removed entirely on 2026-07-14 rather than finished — see below.

## 2026-07-14 — D-Bus-only transport, resume recovery, GUI sunset

- **Removed the loopback WebSocket transport** from `gpservice` and `gpgui`: the
  WS server, connection, routes and handlers, the shared api-key (and stdin key),
  and the pkexec GUI-launch path are deleted. Both now reach the backend only over
  the polkit-gated D-Bus system service (native and Flatpak). `gpservice` always
  runs D-Bus (`--dbus` is accepted for the activation file, and implied).
- **Reliable, fast reconnect after resume from sleep** (`apps/gpservice/src/{vpn_task,gateway_pin}.rs`):
  on resume, re-pin the gateway's host route to the physical NIC (`gateway_pin`)
  and trigger an in-place reconnect (`vpn.pause()`), keeping tun0 up. A NIC flap on
  resume drops openconnect's gateway host route, so its reconnect/logout sockets
  fall back to the dead `tun0` default and hang for the full TCP timeout (~2 min);
  re-pinning restores the physical path so they reconnect in ~1 s. tun0 stays up
  throughout, so nothing can leak — everything but the pinned gateway remains bound
  to the dead tunnel (fail-closed). The gateway route is captured at connect time
  (resolved once while the network is healthy). A tunnel that exits unexpectedly is
  still re-established (bounded retries) instead of dropping to Disconnected.
  Also fixes the VPN state getting stuck on "Reconnecting" after a pause-driven
  resume reconnect: openconnect re-establishes via its fresh-connect path (not the
  internal `ssl_reconnect()`), so neither the reconnected handler nor `setup_tun`
  fires; the C wrapper (`crates/openconnect/src/ffi/vpn.c`) now emits the
  reconnected notification itself when the mainloop returns from a pause, so the
  state flips back to Connected.
- **Smart card:** keep the PKCS#11 (cryptoki) context initialized process-wide so
  repeat connects tolerate an already-initialized module.
- **GUI security/reliability hardening** (`apps/gpgui`): validate update versions
  before the root install script; SO_PEERCRED same-user check on the
  single-instance socket; atomic 0600 vault/config writes; stronger Argon2id vault
  key with transparent migration of existing vaults; drop the hardcoded dev path.
- **Sunset toward GP Client:** `gpgui` shows a "moved to a new app" notice and
  backs up identities once the successor `gp-client` has a public release.

## 2026-07-18 — Dependency refresh + rustls build slimming (GPS-10/11)

- Slimmed the rustls dependency to `default-features = false` with
  `ring/std/tls12/logging` (our PKCS#11 signing already uses the ring provider),
  dropping the default **aws-lc-rs / aws-lc-sys / cmake** backend from the build
  graph entirely.
- Refreshed dependencies on our own cadence (rustls 0.23.40 -> 0.23.42, dbus
  0.9.11 -> 0.9.12, anyhow, chrono, clap, ...) rather than cherry-picking the
  upstream lockfile bump. Inspired by upstream 3817227 (see the GPS-5 review).

## 2026-07-18 — Close the disconnect-vs-reconnect race (GPS-7)

- The openconnect command pipe (`g_cmd_pipe_fd`) is a process global that is
  momentarily stale while the connection thread rebuilds a dropped tunnel, so a
  user disconnect landing in that window wrote its `OC_CMD_CANCEL` to a dead fd
  and was lost — the tunnel then resurrected. New `openconnect::request_cancel()`
  lets the connected callback re-check `user_disconnect` the instant a session
  goes live and cancel it on the (now-live) pipe, complementing the
  top-of-loop check. Together they close the window.

## 2026-07-18 — Bounded disconnect (no more SIGABRT on stop)

- `VpnTaskContext::disconnect` now bounds its wait for openconnect's teardown
  (`DISCONNECT_TIMEOUT`, 5s). A logout POST against a dead gateway session can
  hang the mainloop indefinitely; since the service-shutdown path awaits
  disconnect, systemd would SIGABRT the stop job. On timeout the VPN state is
  forced to Disconnected and the detached connection thread is reaped at process
  exit (GPS-3).

## 2026-07-18 — Retired the WS-era launch plumbing; renamed the SSO-callback handler

- Removed the dead remnants of the pre-D-Bus (WebSocket) transport: the
  `ServiceLauncher` (pkexec-launched gpservice — now D-Bus-activated), the
  `http_endpoint`/`ws_endpoint` helpers and the `/active-gui` probe, the
  `CommandExt::new_pkexec` trait method, and the `GP_SERVICE_BINARY` constant +
  its build.rs env. `gpclient launch-gui` now does only what it is actually
  reached for — delivering the browser SSO `globalprotectcallback:` data to the
  waiting `gpclient connect`.
- Renamed the scheme-handler desktop entry `gpgui.desktop` → `gpclient.desktop`
  (marked `NoDisplay`, generic icon) and removed the now-unused
  `io.github.techneut92.gpgui.policy` pkexec action (the live D-Bus path uses
  `gpservice.policy`). This clears the last gpgui-named artifacts.

## 2026-07-18 — Removed the in-repo gpgui GUI

- Deleted `apps/gpgui` (the bundled Tauri GUI) and stripped it from every
  packaging format: the `-gui` subpackage, the `INCLUDE_GUI` build plumbing, the
  gpgui binary/desktop/icon installs, and the webkit2gtk build/runtime
  dependencies that existed only for its embedded webview. The graphical client
  now lives in its own repository (gp-client, distributed as a Flatpak); this
  repo is the backend (`gpservice` + `gpclient` CLI + `gpauth`) only, and is now
  webkit-free.

## 2026-07-18 — Portal-mode auth handoff (wire-protocol v5)

- **Portal flow in the GUI auth handoff** (`apps/gpservice/src/auth_flow.rs`):
  `ProbeRequest`/`ConnectAuthRequest` gained `as_gateway` (gp-protocol 1.3,
  serde-default true). When false, `build_connect_request` runs the portal path
  — `retrieve_config` for the gateway list, region-preferred gateway selection,
  then `gateway_login` with the portal cookie — instead of treating the server
  as a gateway. This reuses the gpapi portal code that already backs the
  `gpclient` CLI; the direct-gateway path is unchanged. Interactive MFA/token
  challenges during portal auth remain unimplemented (an explicit error).

## 2026-07-18 — Guaranteed DNS restore + scoped-DNS opt-in (wire-protocol v4)

- **DNS restore on every session end** (`apps/gpservice/src/vpn_task.rs`): the
  connection thread's epilogue — and the start of a user disconnect — now run a
  best-effort `resolvectl revert <tundev>` (+ cache flush). Previously only the
  vpnc-script's clean `reason=disconnect` path reverted resolved, so an abnormal
  session death (portal-side logout, hung teardown) left the dead corporate
  resolvers as the system-wide default DNS route until reboot.
- **Scoped-DNS opt-in** (gp-protocol 1.2 / wire v4, `ConnectArgs::dns_domains`):
  gpservice validates the client's domain list and hands it to the vpnc-script as
  `GP_DNS_DOMAINS` (set in the C wrapper `crates/openconnect/src/ffi/vpn.c`,
  inherited by the script child). The script
  (`packaging/files/usr/libexec/gpclient/vpnc-script`,
  `modify_resolved_manager`) then scopes resolved to those domains
  (`resolvectl domain` + `default-route false`) instead of the `~.`
  default-DNS-route fallback, merging any server-provided split-DNS domains.
  Behavior without the opt-in is unchanged.

## 2026-07-18 — Self-healing PKCS#11 reader re-scan (GPS-15)

- **`crates/gpapi/src/utils/pkcs11.rs`:** the process-global cryptoki context
  (`pkcs11_context`, cached in a `OnceLock` so `C_Finalize` never tears the
  module out from under the openconnect/GnuTLS tunnel) now retries its own init
  once. `build_pkcs11_context` reports whether *this* call ran `C_Initialize`
  (vs. finding it already initialised); the new `init_pkcs11_context` wrapper, if
  it owns that init and the module enumerates no token
  (`get_slots_with_token()` empty), drops the context (`C_Finalize`) and rebuilds
  it so the module re-scans readers. This runs inside `get_or_init`, before the
  context is cached and before any tunnel exists, so the re-scan is safe. Fixes a
  permanent "no PKCS#11 token matching …" that stuck for the gpservice lifetime
  when the reader was contended at first init.

## 2026-07-19 — Portal-mode MFA / token challenges (GPS-16 follow-up)

In portal mode the RSA/OTP challenge is normally issued by the *portal* (the
gateway then reuses the portal cookie without re-challenging), but the
interactive-MFA loop added for GPS-16 only wrapped the gateway login — a
challenged portal `getconfig` just errored out. Wired the portal path into the
same challenge machinery; no protocol change (the GUI sees the same
`MfaChallenge` state / `submit_mfa` call regardless of which step challenged):

- **`crates/gpapi/src/portal/config.rs`:** `retrieve_config` now returns
  `PortalConfigResult` — `Config(PortalConfig)` or `Mfa(message, inputStr)`,
  mirroring `GatewayLogin`. A challenged getconfig is detected before XML
  parsing (the portal answers with the same `respStatus = "Challenge"` JS blob
  as the gateway login endpoint) via the now-shared
  `gateway::parse_mfa`.
- **`apps/gpservice/src/auth_flow.rs`:** new `retrieve_config_mfa` loop —
  prompt the GUI through the existing `MfaPrompter`, resubmit with
  `inputStr` + the entered code, repeat until the config comes back (or the
  user cancels); `connect_via_portal` uses it.
- **`apps/gpclient/src/connect.rs`:** the CLI's portal flow gained the same
  loop with an inline prompt (`retrieve_portal_config`), matching its existing
  gateway-MFA handling.

## 2026-07-19 — Portal gateway picker (GatewaySelect / select_gateway)

In portal mode the region-preferred gateway was chosen silently. The connect
pipeline can now hand the choice to the user, mirroring the interactive-MFA
plumbing (gp-protocol 1.5.1 adds `VpnState::GatewaySelect`; the protocol
constants are unchanged — nothing pre-release speaks the old shape):

- **`apps/gpservice/src/auth_flow.rs`:** new `GatewaySlot`/`GatewayPrompter`
  (the `MfaSlot`/`MfaPrompter` pattern). `connect_via_portal`, when the portal
  returns more than one gateway, emits `GatewaySelect` — the same `ConnectInfo`
  shape as `Connecting`, with `gateway` = the region-preferred pick and
  `gateways` = the full sorted list — and parks until the client answers (or
  cancels, which aborts the connect). Single-gateway portals are unchanged.
- **`apps/gpservice/src/dbus_service.rs`:** new `select_gateway(address)`
  method resolving the parked prompt; polkit-gated like `submit_mfa`.
- **`apps/gpservice/src/vpn_task.rs` / `cli.rs`:** the gateway slot is threaded
  beside the MFA slot; `Disconnect` cancels a pending picker too.

## 2026-07-19 — Packaging fixups for the single-package backend

Follow-ups to the gpgui removal and the constants refactor so the deb and Nix
builds succeed for the 1.5.0 backend:

- **deb** (`packaging/deb/rules.in`): with the `-gui` subpackage gone this is a
  single-binary source, so `dh_auto_install` switched its default destdir from
  `debian/tmp/` to `debian/globalprotect-openconnect-dw/` — leaving
  `dh_install`'s file list staring at an empty `debian/tmp` and aborting with
  "missing files, aborting". Added an `override_dh_auto_install` that keeps
  staging in `debian/tmp`, where the `.install` list distributes from.
- **nix** (`flake.nix`): removed the `--replace-fail /usr/bin/gpservice`
  substitution — `crates/common/src/constants.rs` no longer defines a gpservice
  binary path (only `GP_CLIENT_BINARY` / `GP_AUTH_BINARY`), so `--replace-fail`
  aborted `nix build`.

## 2026-07-20 — Portal mode: reliable smart-card re-login + always-on gateway picker

Two portal-mode connect fixes (gateway mode was unaffected):

- **`crates/gpapi/src/utils/pkcs11.rs`:** `create_pkcs11_client_config` opens a
  fresh session and issues `C_Login` per outbound request, and the `Pkcs11`
  context is a process-wide static — so per the PKCS#11 spec the login state is
  shared across every session of the application. Portal mode authenticates
  twice (prelogin, then the portal-config fetch); the second `C_Login` returned
  `CKR_USER_ALREADY_LOGGED_IN` and the connect failed (a retry, which found the
  token logged out again, usually succeeded — so it looked like flakiness). That
  return code is now treated as success: the freshly opened session already
  inherits the token's authenticated state. The login error is also no longer
  mislabelled as a wrong PIN.
- **`apps/gpservice/src/auth_flow.rs`:** the connect-time gateway picker now
  fires for a single-gateway portal too (previously only when the portal offered
  more than one gateway), superseding the "single-gateway portals are unchanged"
  note in the 2026-07-19 entry above.
- **`apps/gpservice/src/vpn_task.rs`:** `connect()` guarded on
  `VpnState::Disconnected`, so the ConnectAuth handoff — which parks on
  `VpnState::GatewaySelect` for the picker — had its tunnel-start request dropped
  after the user picked a gateway (the pick appeared to do nothing). The guard
  now also accepts `GatewaySelect` as a valid mid-auth precursor; any other
  non-Disconnected state (an already-active or connecting tunnel) is still
  ignored.

### Third-party components

This program is GPL-3.0-or-later, a fork of
[yuezk/GlobalProtect-openconnect](https://github.com/yuezk/GlobalProtect-openconnect)
(GPL-3.0). The Flatpak additionally bundles, built from upstream source by the
manifest (both GPL-compatible):

- **pcsc-lite** — BSD-3-Clause — <https://pcsclite.apdu.fr/>
- **OpenSC** — LGPL-2.1-or-later — <https://github.com/OpenSC/OpenSC>

Rust dependencies are MIT/Apache-2.0 (e.g. `reqwest`, `keyring`, `ksni`, `zbus`,
`png`, `gtk`), all compatible with GPL-3.0.
