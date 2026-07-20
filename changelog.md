# Changelog

## 1.5.1 - 2026-07-20

- **Portal-mode smart-card connect no longer fails on the second sign-in.**
  Connecting to an identity configured for *portal mode* with a PKCS#11 smart
  card could silently return to idle with no error — a retry usually worked, so
  it read as flakiness. Portal mode signs in twice (prelogin, then the
  portal-config fetch); because the token's login is shared process-wide, the
  second sign-in hit `CKR_USER_ALREADY_LOGGED_IN` and the connect was abandoned.
  That state is now treated as already-authenticated, so portal-mode connects go
  through on the first try. (Gateway mode signs in once and was never affected.)
- **Picking a gateway now actually starts the tunnel.** When the portal offers
  several gateways, choosing one and pressing Continue previously did nothing —
  the connect was dropped after the pick and the picker just stayed up. (The
  tunnel-start guard rejected the mid-auth picker state; it now accepts it.)

## 1.5.0 - 2026-07-19

- **gpservice now advertises its wire-protocol range.** `gpservice --protocol`
  prints the `PROTOCOL_MIN PROTOCOL_MAX` this build speaks (e.g. `2 4`) and
  exits, without activating the service. This lets a GUI detect a backend that's
  *too new* for it — one speaking a protocol the GUI can't — and show an
  "Update GP Client" screen instead of failing to connect (GPC-42 / GPS-18).
  Older backends lack the flag, which a GUI reads as "compatible".
- **Smart-card prelogin now recovers if the reader was busy at startup.** The
  PKCS#11 module scans readers once, at `C_Initialize`, and gpservice caches
  that context for its whole lifetime. If the card was momentarily contended the
  first time gpservice touched it (GNOME's gsd-smartcard, another pcscd client,
  or the card still settling), the module came up seeing no token and every
  later connect failed with "no PKCS#11 token matching …" until the service was
  restarted — even though the card was plainly present. gpservice now detects an
  empty first enumeration on a context it owns and re-runs
  `C_Finalize`+`C_Initialize` once to re-scan readers, so it self-heals instead
  of staying stuck (GPS-15).
- **Disconnect during an auto-reconnect can no longer be lost.** If you hit
  Disconnect in the brief window while the tunnel was being re-established, the
  cancel could miss the session (the openconnect command pipe is a process
  global that's momentarily stale mid-rebuild), and the tunnel would come back
  up. The connection now re-checks for a pending disconnect the instant a
  session becomes live and cancels it on the current pipe, so the request always
  takes effect (GPS-7).
- **Disconnect can no longer hang the service.** A logout against an
  already-dead gateway session could block openconnect's teardown forever;
  because the shutdown path waits on disconnect, systemd would time out stopping
  the service and kill it with SIGABRT. The teardown wait is now bounded (5s):
  on timeout the state is forced to Disconnected and the hung mainloop is reaped
  on process exit (GPS-3).
- **Removed the in-repo `gpgui` GUI.** The bundled Tauri GUI (`apps/gpgui`) and
  its `-gui` package (and the webkit2gtk dependency it pulled in) are gone. The
  graphical client is now the standalone **GP Client** app
  (<https://github.com/techneut92/gp-client>, shipped as a Flatpak); this repo
  is the backend only — `gpservice`, the `gpclient` CLI, and the `gpauth` SAML
  helper. The backend is now fully webkit-free.
- **Portal mode (experimental).** The auth handoff can now run the portal flow,
  not just direct-gateway: when the connect/probe request sets `as_gateway =
  false` (protocol v5 / gp-protocol 1.3), the backend runs the portal prelogin,
  retrieves the gateway list, picks the region-preferred gateway, and logs into
  it with the portal cookie — reusing the same gpapi code the `gpclient` CLI
  uses. Direct-gateway remains the default and is unchanged. Interactive
  MFA/token challenges are answered in *both* flows: a challenged portal
  getconfig is detected the same way as a challenged gateway login (the portal
  answers with the same `Challenge` response) and prompts through the same
  channel — the GUI's MFA dialog over D-Bus, or an inline prompt in the
  `gpclient` CLI. Portal mode is untested against a live portal — see GPS-6.
- **Gateway picker for portal mode.** When the portal returns more than one
  gateway, gpservice now pauses the connect on a `GatewaySelect` state carrying
  the full gateway list (region-preferred first); the client answers with the
  chosen gateway's address via the new `select_gateway` D-Bus method and the
  login continues. Single-gateway portals connect straight through, and
  cancelling the picker aborts the connect like a cancelled MFA prompt. The
  `gpclient` CLI keeps its own interactive prompt. (gp-protocol 1.5.1; no
  protocol-constant change.)
- **DNS is always restored when a session ends.** gpservice now drops the
  per-link systemd-resolved configuration on the tunnel interface
  (`resolvectl revert` + cache flush) on *every* session end — clean disconnect,
  retries exhausted, or service shutdown — and additionally right when a
  disconnect starts. Previously the revert only ran via the vpnc-script's clean
  teardown path, so an abnormal session death (e.g. a portal-side logout while
  the smart card was absent) could leave the dead corporate resolvers pinned as
  the system-wide default DNS route: a total DNS blackout until reboot (GPS-1).
- **Scoped DNS opt-in (protocol v4).** The connect request can now carry a
  `dns_domains` list. When set, the vpnc-script scopes systemd-resolved to
  those domains (`resolvectl domain` + `default-route false`) instead of the
  `~.` default-DNS-route fallback, so only the listed domains resolve through
  the VPN and everything else stays on the LAN resolvers. Server-provided
  split-DNS domains are merged in; without the opt-in the behavior is unchanged
  (GPS-4). Requires gp-protocol 1.2 (older GUIs/backends interoperate — the
  field is simply absent/ignored).

## 1.4.0 - 2026-07-14

- **D-Bus only transport.** The GUI (gpgui) now reaches the `gpservice` backend
  exclusively over the polkit-gated D-Bus system service — native and Flatpak
  alike. The loopback WebSocket server, its shared api-key, and the pkexec
  GUI-launch path are gone. An active local user connects without a password
  prompt; nothing else can drive the root service.
- **Reliable, fast reconnect after resume from sleep.** On resume the backend
  waits for the NIC to come back, re-pins the gateway's host route to the physical
  NIC, and only then triggers an in-place reconnect. A NIC flap on resume drops
  that route, so openconnect's own reconnect/logout sockets would otherwise fall
  back to the still-present-but-dead `tun0` default route and hang for the full TCP
  timeout (~2 min) even though the physical network is already back; re-pinning
  lets them reach the portal immediately. Because the resume signal arrives before
  the NIC has carrier, the re-pin is retried until it succeeds before reconnecting.
  The tunnel is never torn down, so no traffic can leak during the reconnect
  (everything but the pinned gateway stays bound to the dead tunnel). A tunnel that
  dies unexpectedly is still re-established (bounded retries) rather than dropping
  to Disconnected.
- **Smart card:** tolerate an already-initialized PKCS#11 (cryptoki) module on
  repeat connects — the module is kept initialized process-wide, so back-to-back
  smart-card connects no longer fail.
- **Security & reliability hardening:** update versions are validated before they
  reach the root install script; the single-instance socket only trusts same-user
  peers; the vault and config are written atomically (0600) with a stronger
  Argon2id key (existing vaults auto-migrate — no lost identities); no personal
  paths are baked into the release.
- **Moving to GP Client.** This app is being superseded by **GP Client**, a new
  independent GUI. Once it has a public release, gpgui shows a notice linking to
  it and backs up your identities; GP Client imports your settings on first run.

## 1.3.1 - 2026-07-12

- Backend: `gpservice` gained a versioned **authentication handoff** — prelogin
  (including the smart-card mTLS), SAML, and the gateway login can now run in
  the backend, driven by a GUI over the wire protocol. This is the foundation
  for the upcoming independent **GP Client** GUI (which links no GPL code); the
  current app is unchanged and keeps connecting exactly as before.
- Internal: the GUI↔backend **wire protocol now lives in its own project**,
  [`gp-protocol`](https://github.com/techneut92/gp-protocol), consumed as a
  version-pinned dependency. No user-visible change; protocol compatibility is
  guarded in that project.

## 1.3.0 - 2026-07-12

- Ported relevant improvements from upstream GlobalProtect-openconnect 2.6.x:
  - **SAML sign-in fix**: the portal is now sent a stable `host-id`, which it
    requires to issue the authentication-override cookie — without it, gateway
    SAML sign-in could fail with "saml-auth-status=-1".
  - **Faster CLI reconnects**: `gpclient connect` now caches the portal cookie
    (`~/.config/gpclient/cookie.json`, private file permissions) and reconnects
    straight to the last gateway without re-running SAML. Opt out with
    `--no-cookie-cache`; the cache clears itself when the gateway rejects it.
  - **Browser sign-in is more robust**: `--browser` without a value now
    auto-selects Chrome or Firefox before falling back to the system default;
    sign-in launched from a root context (e.g. `sudo gpclient connect`)
    recovers the desktop session environment so the browser can open; and
    HEAD probes (link previews) no longer consume the single-use sign-in URL.
  - **HIP report**: Microsoft Defender (mdatp) is now detected on Linux and
    reported in the anti-malware section.
- Fixed the **remaining intermittent crash** (segfault, roughly daily) during
  **single sign-on**: the SSO window was created — and, if sign-in took more
  than 10 seconds, revealed — from a background thread, which GTK does not
  allow. Both now run on the UI thread. This was the same class of bug as the
  1.2.11 tray fix and explains why the crash correlated with SSO logins: it
  only struck when the sign-in cookie had expired and the login needed
  interaction.
- Releases now **update the Ubuntu apt repo automatically**: the OBS package
  bump that previously required a manual `osc` commit after each release runs
  as a CI job once the GitHub release is published.
- The client now **notices a dead tunnel right after resume from sleep**:
  instead of claiming "Connected" for minutes while traffic times out, it
  immediately re-establishes the tunnel when the system wakes (no re-login
  needed — the existing session is reused) and shows an honest
  **"Reconnecting…"** state (amber tray animation) while doing so. If the
  session expired during sleep, it goes to Disconnected promptly instead of
  hanging. *(Protocol change: GUI and backend must both be ≥ 1.3.0.)*

## 1.2.11 - 2026-07-12

- Fixed an **intermittent crash** (segfault, roughly once a day in long
  sessions) when the window was revealed from the tray or by a second launch:
  the window was shown from a background thread, which GTK does not allow. The
  reveal now runs on the UI thread.

## 1.2.10 - 2026-07-12

- Fixed the **window not coming to the front** when opened from the tray icon or
  the tray menu's "Open GP Client" (and on a second launch): it now reliably
  raises and takes focus, including on Wayland compositors like COSMIC where a
  plain focus request was being ignored.

## 1.2.9 - 2026-07-08

- Fixed **smart-card reconnects failing** ("data not available") on the Flatpak
  build — after the first connect, later connects failed until the backend
  service was restarted. The backend now restarts cleanly between sessions, so a
  fresh smart-card read is used each time.
- **Disabled accidental zoom** in the app windows (Ctrl+scroll / pinch /
  Ctrl+±), which could distort the fixed-size layout.
- Added **Revolut** and **Ethereum (ETH)** options alongside Ko-fi on the Support
  page — each with a scannable QR code (and a copy-address button for ETH).

## 1.2.8 - 2026-07-07

- Fixed **"Start minimized" being ignored at login**: with the option off, the app
  now shows its window when it autostarts, instead of always starting hidden in the
  tray. The toggle now governs both the login launch and manual launches.

## 1.2.7 - 2026-07-07

- Fixed **"Update all" not updating the backend**: it installed the backend at
  the version the app *currently* was, not the new one, so on an atomic (rpm-ostree)
  system the backend stayed behind until you ran the update a second time. A single
  "Update all" + reboot now moves both the app and the backend to the latest version.
- The backend update is no longer skipped when its installed version can't be read
  (the update is offered instead of silently assuming it's up to date).

## 1.2.6 - 2026-07-07

- Fixed the **relaunch-from-tray crash on Flatpak**: with the window minimized to
  the tray, opening the app again from the launcher crashed the running instance
  (and left the VPN tunnel up). The fix in 1.2.4 didn't work inside the Flatpak
  sandbox; it now reliably reveals the existing window instead.
- The **VPN tunnel is now torn down if the app quits or crashes** while connected,
  so `tun0` is never left up without the app controlling it. Minimizing to the
  tray still keeps the tunnel running as before.

## 1.2.5 - 2026-07-07

- Fixed PKCS#11 (smart-card / YubiKey) connections failing with **"data not
  available"** after the card was re-seated, `pcscd` was restarted, or the
  machine resumed from suspend. The long-running backend now re-reads the token
  before each connection, so it recovers automatically instead of needing a
  service restart.

## 1.2.4 - 2026-07-07

- Fixed a **crash on relaunch**: with the window closed to the tray (app still
  running), opening it again from the launcher would kill the running instance
  instead of showing the window. Relaunching now just reveals the existing
  window.

## 1.2.3 - 2026-06-28

- About: the separate "Update GP Client" and "Update backend" buttons are now a
  single **"Update all"** button next to "Check for updates" — it updates whichever
  of the app and backend is behind, **narrates each step**, and then offers a
  **"Restart now"** button (or "Reboot now", when an atomic backend update needs a
  reboot) to actually apply the update.

## 1.2.2 - 2026-06-28

- About (Flatpak): the **backend's version and install type** are now read from
  the host (via `flatpak-spawn`) instead of the sandbox — they previously showed
  `?` and "Flatpak". The "Update backend" action also targets the host package
  manager when running as a Flatpak.

## 1.2.1 - 2026-06-28

- Fixed the **connection timer** on the main screen — it was stuck at `00:00:00`;
  it now counts up while connected.
- The **tray menu** no longer shows the full error text in its status line (just
  "Error"), so a long message can't blow out the menu width.

## 1.2.0 - 2026-06-28

- The **backend is now webkit-free**. The SAML SSO webview moved entirely into the
  GUI (it runs in-process), so the backend package (`gpservice`/`gpclient`/`gpauth`)
  no longer depends on `libwebkit2gtk` — a leaner install that builds on more
  distros. The graphical client keeps webkit, since it owns the webview now.
- As a side benefit, the GUI now **remembers your SSO session across reconnects**
  within a running session, so reconnecting no longer re-prompts for SSO each time.

## 1.1.2 - 2026-06-28

- Fixed the main-screen **"Update" banner button** doing nothing — it invoked the
  updater without the target version, so it failed silently and the banner hung on
  "Starting update…". It now updates correctly (and shows an error if it can't).
  The Settings → About "Update GP Client" button was unaffected.
- Main window is 10px taller so the update banner no longer makes the content
  scroll.

## 1.1.1 - 2026-06-27

- An **update-available badge** (download icon) appears on the settings gear and
  the About item when a newer release exists for **the app or the backend**, and
  the app **checks for updates on startup**.
- The **About screen** now shows the app and the backend side by side — version,
  install type and update status for each — with separate "Update GP Client" and
  "Update backend" actions.
- Removed the GUI/backend **version-mismatch warning**: compatibility is enforced
  by the wire-protocol handshake at connect, so a mere version difference no longer
  warns.
- Fixed **"Update backend" on atomic / rpm-ostree** systems — it failed with
  *"conflicting requests"* when a backend was already layered; it now replaces the
  old layer first. The button also shows clearer, live progress while it runs.

## 1.1.0 - 2026-06-27

- Reliability: the GUI and backend now **negotiate a wire-protocol version** at
  connect — they settle on the highest version both support, so a newer GUI keeps
  working with an older backend and vice-versa. Only a genuine incompatibility is
  refused, and the message says which side to update (and whether to upgrade or
  downgrade GP Client). Under the hood the GUI↔backend protocol moved into a
  shared crate so the two can no longer drift out of sync.
- The `gpservice` system-service journal is much quieter — the verbose D-Bus
  handshake/dispatch logging is capped at `warn`.

## 1.0.5 - 2026-06-27

- Packaging: **Ubuntu 26.04 apt repo** — the backend (and native `-gui`) can be
  installed and auto-updated from an openSUSE Build Service repo
  (`download.opensuse.org/repositories/home:Techneut92:gp-client/xUbuntu_26.04`).
  Other Debian/Ubuntu keep the prebuilt `.deb` + the Flatpak (older distros ship
  Rust too old to build from source).
- CI: a **deb install smoketest** installs the freshly built `.deb` in a clean
  Ubuntu image (apt resolves the runtime deps) and gates the release — the deb
  counterpart of the rpm install test.
- Docs: the Install section is reorganized **per distro**, each distro's repo and
  manual options together.

## 1.0.4 - 2026-06-27

- Packaging: the **Fedora COPR repo now also covers Enterprise Linux 10** — RHEL 10,
  AlmaLinux 10, Rocky 10, CentOS Stream 10 (via EPEL 10), for x86_64 and aarch64.
  (EL 9 isn't built — its Rust 1.84 is older than the dependency tree needs.)
- Packaging: the Fedora COPR publish now runs from the release pipeline **only
  after the RPM install test passes** — a package that fails the smoke test is
  never published to COPR.
- Docs: added a **Quickstart** to the README — one command to install *or update*
  GP Client to the latest Flatpak release (and a CLI-only pointer to the backend
  package); reordered Install so repo-based installs (COPR) come before the
  manual host-package download.

## 1.0.3 - 2026-06-27
- GUI: the GUI↔backend compatibility warning now only appears on a **feature
  (minor) or breaking (major)** version divergence — patch-level differences (the
  `x` in `vz.y.x`) are treated as compatible and no longer show the mismatch
  banner or the "Update backend" prompt.
- Flatpak: the AppStream listing now includes **screenshots**, so they show on
  the app's page in GNOME Software / KDE Discover.
- Packaging: the **Nix flake builds again** — it now builds the whole workspace,
  including the GUI, from source and is verified in CI
  (`nix build 'git+https://github.com/techneut92/GlobalProtect-openconnect-dw?submodules=1#default'`).
- Packaging: **Fedora COPR** — the backend (and an optional native `-gui`) RPM is
  now built on COPR for each release (Fedora x86_64 + aarch64), installable with
  `dnf copr enable techneut92/globalprotect-openconnect-dw` (or layered with
  `rpm-ostree` on atomic Fedora — see the README).
- CI: an **RPM install test** installs the freshly built package in a clean
  Fedora image (resolving the real runtime deps) and checks it layers cleanly on
  atomic (no install scriptlets, `/usr`-only); the release gates on it.
- Docs: added a Ko-fi support link to the README.

## 1.0.2 - 2026-06-26
- GUI: **in-app update** under Flatpak now downloads the new `.flatpak` from the
  release and reinstalls it (keeps your vault/config) instead of a no-op
  `flatpak update`.
- GUI: tidier About update box — larger text, buttons below the status, and
  "Check for updates" hides once an update is found.

## 1.0.1 - 2026-06-26
- Flatpak: fix SSO — run the bundled `gpauth` (`/app/bin/gpauth`) instead of the
  missing `/usr/bin/gpauth`.
- GUI: show an "Installing…" progress message while the backend installs (after
  the password prompt), instead of leaving the "approve the prompt" message.
- GUI: **Update backend** button in Settings → About — when the backend service
  is older than the GUI, update it to the matching version in one click (native;
  under Flatpak the host backend version isn't readable yet).

## 1.0.0 - 2026-06-26
First release of the fork as **GP Client** — a GlobalProtect-compatible VPN
client with smart-card / PKCS#11 (YubiKey PIV) authentication.


- GUI: redesigned tray icon with colour-coded connection states (disconnected /
  connecting / connected) and an animated connecting state; choose a **Shield**
  or **Signal ring** style in Settings → General.
- GUI: closing the window now minimises to the tray and keeps the VPN running —
  left-click the tray icon to reopen, right-click for Open / Disconnect / Quit
  (Quit also disconnects).
- GUI: new **Run at system startup** option (on by default), launching hidden to
  the tray at login.
- GUI: new application icon (shield-and-globe mark).
- GUI: mark the window so tiling window managers (Pop Shell / i3) float it
  instead of tiling; square dropdown corners to fix a webkit rendering glitch.
- GUI: **Connect with…** submenu in the tray to start a connection with any saved
  identity directly; the tray icon reliably resets after connecting/errors.
- GUI: optional **Remember unlock** — store the master PIN in your keyring
  (GNOME Keyring / KWallet / COSMIC) and auto-unlock on launch.
- GUI: **Start minimized** option, and a **Forgot your PIN?** reset on the unlock
  screen (deletes saved identities, with a warning).
- GUI: redesigned main window (taller, footer Connect/Disconnect button), a
  **Support / Ko-fi** option, and clearer connection errors (the real reason is
  shown instead of a generic message).
- GUI: float reliably in tiling shells by registering an app float rule, and use
  a reverse-DNS app-id (`io.github.techneut92.gpgui`).
- GUI: new **About** tab with an in-app **update check** (startup + on demand,
  with an update banner); it warns if the GUI and backend service versions don't
  match.
- GUI: if the backend service isn't installed, an **install screen** with
  instructions fitting your OS (Flatpak / dnf / rpm-ostree / apt / …) and a
  best-effort install button.
- GUI: **Flatpak** build-ready — manifest with the right permissions (network,
  keyring, tray, host commands, autostart) and a build script — built and
  verified end-to-end, including the system-tray icon.
- GUI: redesigned **backend-install screen** — clear, copyable install commands
  for your distro plus a one-click **Install** button that actually reports
  whether it worked.
- GUI: **"Unlock automatically"** opt-in when creating your vault (stores the PIN
  in your keyring), and the **About** page now shows your real OS and, under
  Flatpak, the runtime it's running on.
- GUI: the create/unlock screens are centered, and Manage-identities / Settings
  only show once the vault is open.

> Fork (`globalprotect-openconnect-dw`) entries are versioned independently from
> upstream's releases below.

## 2.5.4 - 2026-05-08

- Add Alpine/musl gpgui release assets for x86_64 and aarch64.
- Use the musl gpgui asset when the helper or package build runs on musl Linux.

## 2.5.3 - 2026-05-08

- Fix systemd-resolved DNS routing when no split-DNS domains are provided (fix [#604](https://github.com/yuezk/GlobalProtect-openconnect/issues/604)).
- Log whether systemd-resolved uses server-provided split DNS or global VPN DNS.

## 2.5.2 - 2026-05-06

- Experimental: GUI/CLI add session expiry warnings and support automatic session extension when allowed by the portal.
- GUI/CLI: expose additional OpenConnect connection options.
- Fix Linux packages to vendor the `vpnc-script`.
- Remove OpenConnect from the packaging dependency.
- Fix reconnect handling by preserving GlobalProtect cookie fields.
- Fix auth callback data parsing
- Improve NixOS packaging and user-level installation docs.
- Upgrade Rust, Tauri, Vite, frontend, and Rust dependencies.

## 2.5.1 - 2025-12-22

- Fix the `.deb` package installation issue (fix [#563](https://github.com/yuezk/GlobalProtect-openconnect/issues/563))
- GUI: fix the tray icon size issue on GNOME Flashback (fix [#564](https://github.com/yuezk/GlobalProtect-openconnect/issues/564))
- Improve the HIP report generation logic

## 2.5.0 - 2025-12-08

- GUI/CLI: statically link OpenConnect
- GUI/CLI: support configure the client version
- CLI: add the `-S/--script-tun` option to pass traffic to the `--script` handler
- GUI: update the branding to `GP Connect`

## 2.4.7 - 2025-11-12

- Support NixOS package installation
- Fix the VPNC script location on Void Linux
- Upgrade to Rust 1.85
- `--browser` without argument uses the system default browser

## 2.4.6 - 2025-10-15

- GUI: support the default configuration file for GUI client (fix [#492](https://github.com/yuezk/GlobalProtect-openconnect/issues/492))
- GUI: add the option to not reuse the authentication cookies (fix [#540](https://github.com/yuezk/GlobalProtect-openconnect/issues/540))
- GUI: improve the license validation logic (fix [#502](https://github.com/yuezk/GlobalProtect-openconnect/issues/502))
- CLI: support the `--browser remote` option to use the remote browser for authentication ([#544](https://github.com/yuezk/GlobalProtect-openconnect/pull/544) by [@dark12](https://github.com/dark12))
- CLI: fix gpclient disconnect bailing with client is already running issue ([#542](https://github.com/yuezk/GlobalProtect-openconnect/pull/542) by [@zeroepoch](https://github.com/zeroepoch))
- CLI: fix the `--passwd-on-stdin` reads again on gateway failure ([#546](https://github.com/yuezk/GlobalProtect-openconnect/issues/546))

## 2.4.5 - 2025-07-16

- GUI/CLI: fix the issue that the custom port is not supported issue (fix [#404](https://github.com/yuezk/GlobalProtect-openconnect/issues/404))
- CLI: add the `--force-dpd` option to specify the interval for DPD (Dead Peer Detection).
- CLI: add the `-i/--interface` option to specify the interface to use.

## 2.4.4 - 2025-02-09

- GUI: fix multiple tray icons issue (fix [#464](https://github.com/yuezk/GlobalProtect-openconnect/issues/464))
- CLI: check the cli running state before running the `gpclient` command (fix [#447](https://github.com/yuezk/GlobalProtect-openconnect/issues/447))

## 2.4.3 - 2025-01-21

- Do not use static default value for `--os-version` option.

## 2.4.2 - 2025-01-20

- Disconnect the VPN when sleep (fix [#166](https://github.com/yuezk/GlobalProtect-openconnect/issues/166), [#267](https://github.com/yuezk/GlobalProtect-openconnect/issues/267))

## 2.4.1 - 2025-01-09

- Fix the network issue with OpenSSL < 3.0.4
- GUI: fix the Wayland compatibility issue
- Support configure the log level
- Log the detailed error message when network error occurs

## 2.4.0 - 2024-12-26

- Upgrade to Tauri 2.0
- Support Ubuntu 22.04 and later

## 2.3.9 - 2024-11-02

- Enhance the OpenSSL compatibility mode (fix [#437](https://github.com/yuezk/GlobalProtect-openconnect/issues/437))

## 2.3.8 - 2024-10-31

- GUI: support configure the external browser to use for authentication (fix [#423](https://github.com/yuezk/GlobalProtect-openconnect/issues/423))
- GUI: add option to remember the credential (fix [#420](https://github.com/yuezk/GlobalProtect-openconnect/issues/420))
- GUI: fix the credential not saved issue (fix [#420](https://github.com/yuezk/GlobalProtect-openconnect/issues/420))
- CLI: fix the default browser detection issue (fix [#416](https://github.com/yuezk/GlobalProtect-openconnect/issues/416))

## 2.3.7 - 2024-08-16

- Fix the Rust type inference regression [issue in 1.80](https://github.com/rust-lang/rust/issues/125319).

## 2.3.6 - 2024-08-15

- CLI: enhance the `gpauth` command to support external browser authentication
- CLI: add the `--cookie-on-stdin` option to support read the cookie from stdin
- CLI: support usage: `gpauth <portal> --browser <browser> 2>/dev/null | sudo gpclient connect <portal> --cookie-on-stdin`
- CLI: fix the `--browser <browser>` option not working

## 2.3.5 - 2024-08-14

- Support configure `no-dtls` option
- GUI: fix the tray icon disk usage issue (#398)
- CLI: support specify the browser with `--browser <browser>` option (#405, #407, #397)
- CLI: fix the `--os` option not working

## 2.3.4 - 2024-07-08

- Support the Internal Host Detection (fix [#377](https://github.com/yuezk/GlobalProtect-openconnect/issues/377))
- CLI: support pass the password from stdin (fix [#381](https://github.com/yuezk/GlobalProtect-openconnect/issues/381))

## 2.3.3 - 2024-06-23

- GUI: add the remark field for the license activation
- GUI: check the saved secret key length

## 2.3.2 - 2024-06-17

- Fix the CAS callback parsing issue (fix [#372](https://github.com/yuezk/GlobalProtect-openconnect/issues/372))
- CLI: fix the `/tmp/gpauth.html` deletion issue (fix [#366](https://github.com/yuezk/GlobalProtect-openconnect/issues/366))
- GUI: fix the license not working after reboot (fix [#376](https://github.com/yuezk/GlobalProtect-openconnect/issues/376))
- GUI: add the license activation management link

## 2.3.1 - 2024-05-21

- Fix the `--sslkey` option not working

## 2.3.0 - 2024-05-20

- Support client certificate authentication (fix [#363](https://github.com/yuezk/GlobalProtect-openconnect/issues/363))
- Support `--disable-ipv6`, `--reconnect-timeout` parameters (related: [#364](https://github.com/yuezk/GlobalProtect-openconnect/issues/364))
- Use default labels if label fields are missing in prelogin response (fix [#357](https://github.com/yuezk/GlobalProtect-openconnect/issues/357))

## 2.2.1 - 2024-05-07

- GUI: Restore the default browser auth implementation (fix [#360](https://github.com/yuezk/GlobalProtect-openconnect/issues/360))

## 2.2.0 - 2024-04-30

- CLI: support authentication with external browser (fix [#298](https://github.com/yuezk/GlobalProtect-openconnect/issues/298))
- GUI: support using file-based storage when the system keyring is not available.

## 2.1.4 - 2024-04-10

- Support MFA authentication (fix [#343](https://github.com/yuezk/GlobalProtect-openconnect/issues/343))
- Improve the Gateway switcher UI

## 2.1.3 - 2024-04-07

- Support CAS authentication (fix [#339](https://github.com/yuezk/GlobalProtect-openconnect/issues/339))
- CLI: Add `--as-gateway` option to connect as gateway directly (fix [#318](https://github.com/yuezk/GlobalProtect-openconnect/issues/318))
- GUI: Support connect the gateway directly (fix [#318](https://github.com/yuezk/GlobalProtect-openconnect/issues/318))
- GUI: Add an option to use symbolic tray icon (fix [#341](https://github.com/yuezk/GlobalProtect-openconnect/issues/341))

## 2.1.2 - 2024-03-29

- Treat portal as gateway when the gateway login is failed (fix #338)

## 2.1.1 - 2024-03-25

- Add the `--hip` option to enable HIP report
- Fix not working in OpenSuse 15.5 (fix #336, #322)
- Treat portal as gateway when the gateway login is failed (fix #338)
- Improve the error message (fix #327)

## 2.1.0 - 2024-02-27

- Update distribution channel for `gpgui` to complaint with the GPL-3 license.
- Add `mtu` option.
- Retry auth if failed to obtain the auth cookie

## 2.0.0 - 2024-02-05

- Refactor using Tauri
- Support HIP report
- Support pass vpn-slice command
- Do not error when the region field is empty
- Update the auth window icon
