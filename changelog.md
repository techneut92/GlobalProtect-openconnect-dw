# Changelog

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
