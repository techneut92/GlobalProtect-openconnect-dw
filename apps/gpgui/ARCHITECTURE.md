# gpgui architecture

`gpgui` is the unprivileged Tauri (HTML/JS + Rust) front-end. The privileged VPN
tunnel runs in **`gpservice`** (root, openconnect + tun). They talk over an
encrypted channel; the GUI never has root.

```
 ┌─────────────────────────── unprivileged (user) ───────────────────────────┐
 │  gpgui (Tauri)                                                             │
 │   webview (HTML/JS)  ──invoke──►  Rust commands ──► vpn::run thread        │
 │                                                     │                      │
 │   1. auth (prelogin mTLS + SAML) via gpauth / browser  ──► cookie          │
 │   2. ConnectRequest{info, args{cookie, certificate}}                       │
 └──────────────────────────────────┬────────────────────────────────────────┘
                                     │  encrypted transport (api-key)
 ┌──────────────────────────────────▼──────────── root ───────────────────────┐
 │  gpservice  ──►  openconnect  ──►  tun0                                      │
 └─────────────────────────────────────────────────────────────────────────────┘
```

## The transport seam (and why it matters for Flatpak)

Everything the GUI does *above* the transport is deployment-agnostic. The only
deployment-specific part is **how the GUI reaches gpservice**, isolated to two
spots:

- `client.rs` — connect + encrypted framing
- `vpn::ensure_service` — make sure gpservice is running + get an endpoint

### Native packages (.deb/.rpm/.apk) — implemented
- GUI launches gpservice via `pkexec /usr/bin/gpservice --api-key-on-stdin`
  (passwordless through the shipped polkit rule), piping a per-user 32-byte key.
- IPC: loopback WebSocket on `127.0.0.1:<port>`, port from
  `/var/run/gpservice.lock`. Frames are ChaCha20-Poly1305 with the shared key.
- gpservice is tied to the GUI's lifetime (exits ~3s after the last client
  disconnects).

### Flatpak GUI + host backend — planned (the D-Bus path)
A Flatpak sandbox **cannot** `pkexec`, see `/var/run`, or create a tun. So:
- **gpservice ships as a host package** (.deb/.rpm/.apk) and runs as a
  **D-Bus *system* service**, polkit-activated on first use (replacing the
  pkexec-launch + lockfile model).
- The Flatpak GUI reaches it over the **system bus** with
  `--system-talk-name=<bus name>` (no `--share=network`, no host FS access).
- The api-key handshake is replaced by polkit authorization on the D-Bus method
  calls; ConnectRequest/VpnState become D-Bus method + signal payloads.

**Plugging it in:** add a `ServiceTransport` trait with two impls —
`LoopbackWs` (today) and `DbusSystem` (Flatpak) — chosen at runtime
(e.g. detect `/.flatpak-info`). Nothing in the auth pipeline or UI changes.

## Packaging matrix
| Component | deb | rpm | apk | flatpak |
|---|---|---|---|---|
| gpgui (GUI) | ✓ | ✓ | ✓ | ✓ |
| gpservice / gpclient / gpauth | ✓ | ✓ | ✓ | ✗ (needs root + tun) |

## Auth (SSO)
Chosen in **Advanced → Auth → SSO method**:
- **Embedded webview** — spawns `gpauth` (webkit2gtk). In a Flatpak the runtime
  provides webkit.
- **System browser** — `gpauth --default-browser`: opens the real browser +
  loopback callback. No webkit; best with password managers / passkeys; and the
  natural choice in a sandbox (open via the `OpenURI` portal).

The pkcs11 prelogin (smart card) always runs in the Rust backend — the webview
never touches the token.
