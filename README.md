# GlobalProtect openconnect (dw)

[![Release](https://img.shields.io/github/v/release/techneut92/GlobalProtect-openconnect-dw?label=release&color=brightgreen)](https://github.com/techneut92/GlobalProtect-openconnect-dw/releases/latest)
[![Copr build status](https://copr.fedorainfracloud.org/coprs/techneut92/globalprotect-openconnect-dw/package/globalprotect-openconnect-dw/status_image/last_build.png)](https://copr.fedorainfracloud.org/coprs/techneut92/globalprotect-openconnect-dw/package/globalprotect-openconnect-dw/)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0--or--later-blue.svg)](./LICENSE)
[![Downloads](https://img.shields.io/github/downloads/techneut92/GlobalProtect-openconnect-dw/total?label=downloads&color=blue)](https://github.com/techneut92/GlobalProtect-openconnect-dw/releases)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-support-FF5E5B?logo=ko-fi&logoColor=white)](https://ko-fi.com/techneut92)

A GlobalProtect-compatible VPN client for Linux with **smart-card / PKCS#11
(YubiKey PIV) certificate authentication** — alongside SAML single sign-on and
username/password. Built on [OpenConnect](https://www.infradead.org/openconnect/),
this repository is the **command-line client** (`gpclient`), the **privileged
backend service** (`gpservice`, a D-Bus daemon), and the `gpauth` SAML helper.

A fork of [yuezk/GlobalProtect-openconnect](https://github.com/yuezk/GlobalProtect-openconnect),
**fully open source under the GPL-3.0**.

> **This repository is CLI + service only** — the command-line client
> (`gpclient`), the privileged D-Bus service (`gpservice`), and the `gpauth`
> SAML helper. It has **no GUI** and is **webkit-free**.
>
> The graphical desktop app, **GP Client**, is a separate side project at
> [github.com/techneut92/gp-client](https://github.com/techneut92/gp-client)
> (shipped as a Flatpak). This repository is the privileged backend that GP
> Client installs and drives over D-Bus — you can also use it entirely on its
> own from the command line.

<p align="center">
  <img width="380" src="docs/screenshots/gp-client-backend-required.png" alt="GP Client prompting to install this backend service">
</p>
<p align="center"><em>GP Client (the desktop app) installs and drives this backend — this repository is the CLI + service it needs.</em></p>

> **GlobalProtect** is a trademark of Palo Alto Networks. This is an independent,
> compatible client and is not affiliated with or endorsed by Palo Alto Networks.

## Contents

- [Quickstart](#quickstart)
- [Features](#features)
- [Install](#install)
- [Usage](#usage)
- [Building from source](#building-from-source)
- [Distribution roadmap](#distribution-roadmap)
- [License](#license)

## Quickstart

Install the backend service and CLI. On **Fedora**:

```bash
sudo dnf copr enable techneut92/globalprotect-openconnect-dw
sudo dnf install globalprotect-openconnect-dw
```

Re-running `dnf install` updates you to the latest release. See
[Install](#install) for other distros and the manual packages, and
[Usage](#usage) to connect.

Prefer a desktop app? **GP Client** — the graphical client — is developed
separately and ships as a Flatpak:
[github.com/techneut92/gp-client](https://github.com/techneut92/gp-client). It
drives this backend over D-Bus.

## Features

- **Smart-card / PKCS#11 auth** — YubiKey PIV (or any PKCS#11 token) client
  certificate for portal *and* gateway login.
- **SAML SSO** — via the system browser (`gpauth`), plus username/password.
- **Scriptable CLI** — `gpclient` connect/disconnect, fully scriptable.
- Multi-portal and direct-gateway connections.

> The desktop **GP Client** app (a separate Flatpak) layers an encrypted
> identity vault, keyring unlock (GNOME Keyring / KWallet / COSMIC), a system
> tray, and start-at-login on top of this backend.

## Install

The backend is distributed via **[GitHub Releases](https://github.com/techneut92/GlobalProtect-openconnect-dw/releases)**
and distro repos as a **host package** — the privileged, **webkit-free** helper
that brings up the tunnel (`gpservice` + `gpclient` + `gpauth`). It needs root +
the TUN device, so it must be a host package (`.rpm`/`.deb`/`.pkg.tar.zst`/`.apk`).

> Looking for the desktop app? **GP Client** ships separately as a Flatpak —
> see [github.com/techneut92/gp-client](https://github.com/techneut92/gp-client).
> It talks to this backend over D-Bus.

### Fedora

Install (and auto-update) from COPR:

```bash
sudo dnf copr enable techneut92/globalprotect-openconnect-dw
sudo dnf install globalprotect-openconnect-dw
```

**Atomic** (Silverblue / Kinoite / Bazzite / Bluefin — no `dnf copr`): add the repo
file, layer it, and reboot:

```bash
fed=$(rpm -E %fedora)
sudo curl -fsSL -o /etc/yum.repos.d/_copr_techneut92-gpoc-dw.repo \
  "https://copr.fedorainfracloud.org/coprs/techneut92/globalprotect-openconnect-dw/repo/fedora-$fed/techneut92-globalprotect-openconnect-dw-fedora-$fed.repo"
rpm-ostree install globalprotect-openconnect-dw
systemctl reboot
```

Or grab the `.rpm` from the
[release](https://github.com/techneut92/GlobalProtect-openconnect-dw/releases):

```bash
sudo dnf install ./globalprotect-openconnect-dw-*.rpm
```

---

### RHEL / AlmaLinux / Rocky / CentOS Stream 10

The same COPR repo builds for Enterprise Linux 10 via EPEL:

```bash
sudo dnf install epel-release
sudo dnf copr enable techneut92/globalprotect-openconnect-dw
sudo dnf install globalprotect-openconnect-dw
```

Or a manual `.rpm` from the
[release](https://github.com/techneut92/GlobalProtect-openconnect-dw/releases):

```bash
sudo dnf install ./globalprotect-openconnect-dw-*.rpm
```

> EL **9** isn't built — its Rust (1.84) is older than the dependency tree needs
> (≥ 1.88).

---

### Debian / Ubuntu

**Ubuntu 26.04** — install (and auto-update) from the apt repo:

```bash
. /etc/os-release   # uses VERSION_ID, e.g. 26.04
sudo mkdir -p /etc/apt/keyrings
curl -fsSL "https://download.opensuse.org/repositories/home:Techneut92:gp-client/xUbuntu_$VERSION_ID/Release.key" \
  | gpg --dearmor | sudo tee /etc/apt/keyrings/gp-client.gpg >/dev/null
echo "deb [signed-by=/etc/apt/keyrings/gp-client.gpg] https://download.opensuse.org/repositories/home:Techneut92:gp-client/xUbuntu_$VERSION_ID/ /" \
  | sudo tee /etc/apt/sources.list.d/gp-client.list
sudo apt update && sudo apt install globalprotect-openconnect-dw
```

**Any other Debian/Ubuntu** — download the `.deb` from the
[release](https://github.com/techneut92/GlobalProtect-openconnect-dw/releases) and
install it directly. The prebuilt package runs on **Debian 12+ and Ubuntu 22.04+**
(glibc ≥ 2.34):

```bash
sudo apt install ./globalprotect-openconnect-dw_*.deb
```

> Only Ubuntu 26.04 has an apt repo (its Rust is new enough to build from
> source); on older Debian/Ubuntu use the manual `.deb`.

---

### Arch

```bash
sudo pacman -U ./globalprotect-openconnect-dw-*.pkg.tar.zst
```

---

### Alpine

```bash
sudo apk add --allow-untrusted ./globalprotect-openconnect-dw-*.apk
```

---

A generic `…bin.tar.xz` is attached to every release for a manual,
distro-agnostic install.

## Usage

The desktop app (**GP Client**) is documented in its own repository —
[github.com/techneut92/gp-client](https://github.com/techneut92/gp-client). Below
is the command-line client.

### Command line (`gpclient`)

The tunnel runs as root, so `connect`/`disconnect` need `sudo`.

```
Usage: gpclient [OPTIONS] <COMMAND>

Commands:
  connect     Connect to a portal (or gateway) server
  disconnect  Disconnect from the server
  launch-gui  Handle the browser-SSO `globalprotectcallback:` URL (scheme handler)
  hip         Generate a HIP report
  help        Print this message or the help of the given subcommand(s)

Global options:
      --ignore-tls-errors   Ignore TLS certificate errors
      --fix-openssl         Extended OpenSSL compatibility mode
  -v, --verbose...          -v debug, -vv trace
  -q, --quiet...            -q warnings, -qq errors
```

#### `gpclient connect <SERVER> [OPTIONS]`

By default `<SERVER>` is treated as a **portal** (prelogin → gateway list →
gateway login); pass `--as-gateway` to connect straight to a gateway.

| Option | Purpose |
| --- | --- |
| `-g, --gateway <NAME>` | Pick a gateway (otherwise prompts in portal mode) |
| `--as-gateway` | Treat `<SERVER>` as a gateway, not a portal |
| `-u, --user <USER>` | Username (prompts if omitted) |
| `--passwd-on-stdin` | Read the password from stdin |
| `-c, --certificate <CERT>` | Client cert: a PEM/PKCS#12 file **or a `pkcs11:` URI** (smart card) |
| `-k, --sslkey <KEY>` · `-p, --key-password <PW>` | Separate key file / passphrase |
| `--browser [BROWSER]` | External-browser SSO — auto / `default` / `remote` (headless) / path |
| `--cookie-on-stdin` · `--cookie-file <PATH>` · `--no-cookie-cache` | Portal-cookie handling |
| `--hip [SCRIPT]` · `--hip-user <USER>` | Host Integrity Protection reporting |
| `-s, --script <FILE>` · `-i, --interface <IFNAME>` · `-S, --script-tun` | Tunnel routing / device |
| `-m, --mtu <N>` · `--disable-ipv6` · `--reconnect-timeout <SECS>` | Tunnel tuning |
| `--os <Linux\|Windows\|Mac>` · `--os-version` · `--client-version` · `--user-agent` | Reported client identity |

`gpclient disconnect [--wait <SECS>]` tears the tunnel down.

#### Examples

```bash
# Smart card (YubiKey PIV) against a gateway — the fork's headline feature:
sudo gpclient connect gp.acme.example --as-gateway \
  --certificate 'pkcs11:manufacturer=piv_II;id=%03;type=cert'

# Portal mode, pick a gateway non-interactively:
sudo gpclient connect gp.acme.example --gateway "Gateway-EU"

# Username / password (password from stdin, never argv):
echo "$PW" | sudo gpclient connect gp.acme.example --user you --passwd-on-stdin

# External-browser SSO (SAML):
sudo -E gpclient connect gp.acme.example --browser
# …or pipe the cookie from gpauth:
gpauth gp.acme.example --browser 2>/dev/null | sudo gpclient connect gp.acme.example --cookie-on-stdin

sudo gpclient disconnect
```

Use `--browser remote` on headless hosts to get a URL you complete elsewhere.

## Building from source

### CLI + backend

Prerequisites: [Rust 1.89+](https://www.rust-lang.org/tools/install) and
OpenConnect build deps (`autoconf`, `automake`, `libtool`, `pkg-config`,
`libxml2`, `gnutls`, `p11-kit`, `nettle`, `gmp`, `zlib`, `lz4` dev packages),
plus `pkexec`.

```bash
git clone https://github.com/techneut92/GlobalProtect-openconnect-dw.git
cd GlobalProtect-openconnect-dw
git submodule update --init --recursive
make build           # or: cargo build --release -p gpclient -p gpservice -p gpauth
sudo make install
```

Build options: `OFFLINE=1` (vendored deps). A DevContainer
(`.devcontainer/`) is provided for a reproducible toolchain.

## Distribution roadmap

Release artifacts are produced automatically by CI on each `vX.Y.Z` tag
(`.github/workflows/build.yaml`) and attached to the GitHub release. Wider
distribution channels to set up as the project matures:

- [x] **GitHub Releases** — `.rpm` / `.deb` / `.pkg.tar.zst` / `.apk` / `.bin.tar.xz`
- [x] **Fedora COPR** — backend `.rpm`, built & published from the
      release pipeline (gated on the RPM install test). Live:
      `dnf copr enable techneut92/globalprotect-openconnect-dw`. Also covers
      RHEL / AlmaLinux / Rocky / CentOS where their Rust is new enough (see note).
- [ ] **Arch AUR** — backend
- [ ] **Debian/Ubuntu PPA / openSUSE OBS** — *constrained:* the dependency tree
      needs **Rust ≥ 1.88**, so source-build services only work on distros that
      ship a recent Rust (Fedora, openSUSE Tumbleweed, the newest EL/Ubuntu).
      Debian ≤13, Ubuntu LTS, and EL9 ship older Rust and can't build from source
      — those users should use the prebuilt `.deb`/`.rpm` from GitHub Releases.
- [ ] **NixOS flake** — `flake.nix` builds the whole workspace from
      source and is checked in CI (`.github/workflows/nix.yaml`); use the git
      fetcher so the submodules come along (the `github:` shorthand omits them):
      `nix build 'git+https://github.com/techneut92/GlobalProtect-openconnect-dw?submodules=1#default'`
- [ ] **Docker image** — CLI-only (`gpclient`/`gpauth`); CI job present, publish disabled

## Support

If this project saves you some time, you can support its development:

- **Ko-fi** — [ko-fi.com/techneut92](https://ko-fi.com/techneut92) (one-off tips, no account needed)
- **Revolut** — [revolut.me/techneut92](https://revolut.me/techneut92)
- **Ethereum (ETH)** — `0x15d9B8383A7cbe9f99F72aC29106C53bbcf4ea40` (Ethereum network; ETH / ERC-20 only)

[![ko-fi](https://img.shields.io/badge/Support%20me%20on-Ko--fi-FF5E5B?logo=ko-fi&logoColor=white)](https://ko-fi.com/techneut92)
[![Revolut](https://img.shields.io/badge/Revolut-tip-0666EB?logo=revolut&logoColor=white)](https://revolut.me/techneut92)

## License

**GPL-3.0-or-later.** © 2026 Dylan Westra (techneut92). A fork of
[yuezk/GlobalProtect-openconnect](https://github.com/yuezk/GlobalProtect-openconnect)
(GPL-3.0). See [LICENSE](./LICENSE) and [CHANGES.md](./CHANGES.md) (GPLv3 §5a
change notices).

| Component | License |
|-----------|---------|
| [gpservice](./apps/gpservice) | [GPL-3.0](./apps/gpservice/LICENSE) |
| [gpclient](./apps/gpclient) | [GPL-3.0](./apps/gpclient/LICENSE) |
| [gpauth](./apps/gpauth) | [GPL-3.0](./apps/gpauth/LICENSE) |
| [gpapi](./crates/gpapi) · [common](./crates/common) · [auth](./crates/auth) · [openconnect](./crates/openconnect) | [GPL-3.0](./LICENSE) |
</content>
</invoke>
