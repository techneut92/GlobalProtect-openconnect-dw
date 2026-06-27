# GP Client

[![Copr build status](https://copr.fedorainfracloud.org/coprs/techneut92/globalprotect-openconnect-dw/package/globalprotect-openconnect-dw/status_image/last_build.png)](https://copr.fedorainfracloud.org/coprs/techneut92/globalprotect-openconnect-dw/package/globalprotect-openconnect-dw/)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](./LICENSE)

A GlobalProtect-compatible VPN client for Linux with **smart-card / PKCS#11
(YubiKey PIV) certificate authentication** — alongside SAML single sign-on and
username/password. Built on [OpenConnect](https://www.infradead.org/openconnect/),
it ships a command-line client and a graphical app (**GP Client**).

A fork of [yuezk/GlobalProtect-openconnect](https://github.com/yuezk/GlobalProtect-openconnect),
**fully open source under the GPL-3.0** (the upstream GUI was proprietary; this
GUI is a clean, open Tauri rewrite).

> **GlobalProtect** is a trademark of Palo Alto Networks. This is an independent,
> compatible client and is not affiliated with or endorsed by Palo Alto Networks.

<p align="center">
  <img width="440" src="docs/screenshots/main.png" alt="GP Client — connect with a smart-card identity">
</p>
<p align="center"><em>Connect with a smart-card (PKCS#11 / YubiKey PIV) identity.</em></p>

<details>
<summary><b>More screenshots</b></summary>

<p align="center">
  <img width="320" src="docs/screenshots/identity_manager.png" alt="Identity manager — PKCS#11 module + YubiKey certificate">
  <img width="320" src="docs/screenshots/settings_about.png" alt="About — host OS, Flatpak runtime, and backend status">
</p>
<p align="center"><em>Identity manager (PKCS#11 / YubiKey) · About (host OS, Flatpak runtime, backend status).</em></p>

<p align="center">
  <img width="240" src="docs/screenshots/set_master_pin.png" alt="Encrypted vault — set a master PIN">
  <img width="240" src="docs/screenshots/backend_required.png" alt="Guided backend install">
  <img width="320" src="docs/screenshots/settings_general.png" alt="Settings — startup & tray">
</p>
<p align="center"><em>Encrypted vault · guided backend install · settings.</em></p>

</details>

## Contents

- [Features](#features)
- [Install](#install)
- [Usage](#usage)
- [Building from source](#building-from-source)
- [Distribution roadmap](#distribution-roadmap)
- [License](#license)

## Features

- **Smart-card / PKCS#11 auth** — YubiKey PIV (or any PKCS#11 token) client
  certificate for portal *and* gateway login.
- **SAML SSO** (embedded webview or external browser) and username/password.
- **Encrypted identity vault** — save multiple connections, optionally unlocked
  from your keyring (GNOME Keyring / KWallet / COSMIC).
- **System tray** with state-aware icons, connect-from-tray, and notifications.
- **CLI and GUI** — the CLI (`gpclient`) is fully scriptable; the GUI (GP Client)
  is an unprivileged app that drives a small privileged host service.
- Multi-portal / direct-gateway, auto-connect at login, start-minimized.

## Install

GP Client is distributed via **[GitHub Releases](https://github.com/techneut92/GlobalProtect-openconnect-dw/releases)**.
It has two parts:

| Part | What it is | How to get it |
|------|------------|---------------|
| **GP Client** (GUI) | The unprivileged app | `.flatpak` bundle (recommended) |
| **Backend service** | Privileged helper that brings up the tunnel (`gpservice` + `gpclient` + `gpauth`) | host package (`.rpm`/`.deb`/`.pkg.tar.zst`/`.apk`) |

The GUI talks to the backend over D-Bus, so the backend must be a **host
package** (it needs root + the TUN device). The app's "backend not installed"
screen shows the exact command for your distro.

### GUI — Flatpak (recommended)

```bash
flatpak install --user io.github.techneut92.gpgui.flatpak
flatpak run io.github.techneut92.gpgui
```

### Backend service — host package

Download `globalprotect-openconnect-dw-<version>…` for your distro and install
the file directly:

```bash
# Fedora / RHEL            sudo dnf install ./globalprotect-openconnect-dw-*.rpm
# Atomic (Silverblue/…)    sudo rpm-ostree install ./globalprotect-openconnect-dw-*.rpm   # then reboot
# Debian / Ubuntu          sudo apt install ./globalprotect-openconnect-dw_*.deb
# Arch                     sudo pacman -U ./globalprotect-openconnect-dw-*.pkg.tar.zst
# Alpine                   sudo apk add --allow-untrusted ./globalprotect-openconnect-dw-*.apk
```

The `…-gui` package and generic `…bin.tar.xz` are also attached for a fully
native (non-Flatpak) install.

### Backend — Fedora COPR

On Fedora, the backend can be installed (and kept updated) from COPR:

```bash
sudo dnf copr enable techneut92/globalprotect-openconnect-dw
sudo dnf install globalprotect-openconnect-dw
# optional native (non-Flatpak) GUI:
sudo dnf install globalprotect-openconnect-dw-gui
```

On **atomic** Fedora (Silverblue / Kinoite / Bazzite / Bluefin — no `dnf copr`),
add the repo file and layer it, then reboot:

```bash
fed=$(rpm -E %fedora)
sudo curl -fsSL -o /etc/yum.repos.d/_copr_techneut92-gpoc-dw.repo \
  "https://copr.fedorainfracloud.org/coprs/techneut92/globalprotect-openconnect-dw/repo/fedora-$fed/techneut92-globalprotect-openconnect-dw-fedora-$fed.repo"
rpm-ostree install globalprotect-openconnect-dw   # add -gui too for the native GUI
systemctl reboot
```

The package ships no install scriptlets and writes only under `/usr`, so it
layers cleanly with `rpm-ostree`.

## Usage

### Graphical (GP Client)

Launch it from your application menu, or:

```bash
flatpak run io.github.techneut92.gpgui      # Flatpak
gpgui                                        # native install
```

Create a vault, add an identity (portal + auth method / PKCS#11 module), and
connect. Manage saved identities and advanced options from the in-app Settings.

### Command line (`gpclient`)

```
Usage: gpclient [OPTIONS] <COMMAND>

Commands:
  connect     Connect to a portal server
  disconnect  Disconnect from the server
  launch-gui  Launch the GUI
  help        Print this message or the help of the given subcommand(s)
```

External-browser SSO:

```bash
sudo -E gpclient connect --browser <portal>
# or, piping the cookie:
gpauth <portal> --browser 2>/dev/null | sudo gpclient connect <portal> --cookie-on-stdin
```

Use `--browser remote` on headless hosts to get a URL you complete elsewhere.

## Building from source

### GUI (Flatpak)

```bash
apps/gpgui/packaging/flatpak/flatpak-build.sh
```

This installs the GNOME 50 runtime/SDK, vendors the cargo registry, and builds
`io.github.techneut92.gpgui` via flatpak-builder (needs `flatpak` +
`flatpak-builder`; on atomic Fedora: `flatpak install flathub org.flatpak.Builder`).

### CLI + backend + native GUI

Prerequisites: [Rust 1.89+](https://www.rust-lang.org/tools/install), Tauri
deps, and OpenConnect build deps (`autoconf`, `automake`, `libtool`,
`pkg-config`, `libxml2`, `gnutls`, `p11-kit`, `nettle`, `gmp`, `zlib`, `lz4`
dev packages), plus `pkexec` and `gnome-keyring`/`pam_kwallet`.

```bash
git clone https://github.com/techneut92/GlobalProtect-openconnect-dw.git
cd GlobalProtect-openconnect-dw
git submodule update --init --recursive
make build           # or: cargo build --release -p gpclient -p gpservice -p gpauth -p gpgui
sudo make install
```

Build options: `OFFLINE=1` (vendored deps). A DevContainer
(`.devcontainer/`) is provided for a reproducible toolchain.

## Distribution roadmap

Release artifacts are produced automatically by CI on each `vX.Y.Z` tag
(`.github/workflows/build.yaml`) and attached to the GitHub release. Wider
distribution channels to set up as the project matures:

- [x] **GitHub Releases** — `.rpm` / `.deb` / `.pkg.tar.zst` / `.apk` / `.bin.tar.xz` + `.flatpak` bundle
- [ ] **Flathub** — submit `io.github.techneut92.gpgui` (AppStream metainfo is in
      `apps/gpgui/packaging/flatpak/`)
- [ ] **Fedora COPR** — backend (+ native `-gui`) `.rpm`; CI auto-submits the
      SRPM on each tag (`.github/workflows/copr.yaml`) — pending project + token
- [ ] **Arch AUR** — backend + GUI
- [ ] **Debian/Ubuntu PPA**
- [ ] **openSUSE OBS**
- [ ] **NixOS flake** — `flake.nix` builds the whole workspace (incl. GUI) from
      source and is checked in CI (`.github/workflows/nix.yaml`); use the git
      fetcher so the submodules come along (the `github:` shorthand omits them):
      `nix build 'git+https://github.com/techneut92/GlobalProtect-openconnect-dw?submodules=1#default'`
- [ ] **Docker image** — CLI-only (`gpclient`/`gpauth`); CI job present, publish disabled

## Support

If this project saves you some time, you can support its development on Ko-fi:

[![ko-fi](https://img.shields.io/badge/Support%20me%20on-Ko--fi-FF5E5B?logo=ko-fi&logoColor=white)](https://ko-fi.com/techneut92)

## License

**GPL-3.0-or-later.** © 2026 Dylan Westra (techneut92). A fork of
[yuezk/GlobalProtect-openconnect](https://github.com/yuezk/GlobalProtect-openconnect)
(GPL-3.0). See [LICENSE](./LICENSE) and [CHANGES.md](./CHANGES.md) (GPLv3 §5a
change notices).

| Component | License |
|-----------|---------|
| [gpgui](./apps/gpgui) (GP Client GUI) | [GPL-3.0](./apps/gpgui/LICENSE) |
| [gpservice](./apps/gpservice) | [GPL-3.0](./apps/gpservice/LICENSE) |
| [gpclient](./apps/gpclient) | [GPL-3.0](./apps/gpclient/LICENSE) |
| [gpauth](./apps/gpauth) | [GPL-3.0](./apps/gpauth/LICENSE) |
| [gpapi](./crates/gpapi) · [common](./crates/common) · [auth](./crates/auth) · [openconnect](./crates/openconnect) | [GPL-3.0](./LICENSE) |

The Flatpak additionally bundles **pcsc-lite** (BSD-3-Clause) and **OpenSC**
(LGPL-2.1+), built from upstream source; their license texts ship in
`/app/share/licenses/`.
