# GlobalProtect-openconnect — PKCS#11 / smart-card fork

Fork of yuezk/GlobalProtect-openconnect adding **client-certificate (smart-card)
auth for the portal/gateway prelogin mTLS**, which upstream only supports as a
PEM/P12 *file*. The portal (`gp.teleperformance.nl`) requires a client cert at
prelogin; the key lives on a YubiKey (PIV, non-extractable).

Branch: `pkcs11-support`

## What was added

- **`--certificate pkcs11:<uri>`** — sign the prelogin mTLS on a PKCS#11 token.
  The prelogin uses reqwest+native-tls (can't carry a pkcs11 key), so for pkcs11
  we build a **rustls `ClientConfig` with a `cryptoki`-backed signing key** and
  feed it via `use_preconfigured_tls`. (`crates/gpapi/src/utils/pkcs11.rs`)
- **`--certificate winsign:<thumbprint>`** — WSL workaround: sign via Windows
  `powershell.exe` (CNG) against a cert in the Windows store. Lets the YubiKey
  stay on Windows, no USB passthrough. (`crates/gpapi/src/utils/winsign.rs`)
- Tunnel: `openconnect` understands `pkcs11:` natively (cert passed through), but
  **not** `winsign:` — so winsign certs are dropped before openconnect and the
  tunnel goes cookie-only. (`apps/gpclient/src/connect.rs`)

## Status (validated on WSL)

- pkcs11 signer: crypto-validated locally via SoftHSM (OpenSSL `verify return:1`).
- winsign signer: **connected to `gp.teleperformance.nl` end-to-end** — cert
  prelogin → SSO/MFA → gateway SAML → ESP tunnel up.
- Note: portal + gateway each need their own SAML here → use **`--as-gateway`**
  to go straight to the gateway (one SSO, fewer PIN prompts), like the old
  `gp-saml-gui --gateway` did.

---

## TODO — test on the actual Linux machine (YubiKey plugged in directly)

On native Linux the card is a real PIV smart card → use the **pkcs11** path
(winsign is WSL-only). No `usbipd` needed.

- [ ] **Prereqs:** `sudo dnf install -y pcsc-lite pcsc-lite-ccid opensc gnutls-utils`
      (apt: `pcscd opensc gnutls-bin`); `sudo systemctl enable --now pcscd`.
- [ ] **Confirm the YubiKey PIV is visible:**
      `p11tool --list-tokens` → find the PIV token;
      `p11tool --list-all-certs 'pkcs11:manufacturer=piv_II'` → cert URI;
      `p11tool --list-privkeys --login 'pkcs11:manufacturer=piv_II'` → key URI.
- [ ] **Build the fork:** need `webkit2gtk-4.1-devel`, `openconnect-devel`,
      `openssl-devel`, `libxdo-devel`, `librsvg2-devel`, `gcc-c++`, Rust 1.89.
      `cargo build -p gpclient -p gpauth -p gpservice`
- [ ] **Connect (pkcs11, as-gateway):**
      ```
      sudo -E target/debug/gpclient connect --as-gateway \
        --os Windows --user-agent 'PAN GlobalProtect' \
        --certificate 'pkcs11:manufacturer=piv_II;type=cert?pin-value=XXXXXX' \
        gp.teleperformance.nl
      ```
      (drop `pin-value=` to be prompted once the `--cert-pin` flag lands)
- [ ] **Drop `--ignore-tls-errors`** — only needed for the SoftHSM/dummy tests;
      the real portal cert should verify against system roots.
- [ ] **Verify tunnel:** `ip -br a` shows a `tun`/`gpd` iface + VPN IP;
      reach an internal host.
- [ ] **Check whether the *tunnel* needs the cert on native** (it was cookie-only
      on WSL). If the gateway demands it, openconnect can use `-c pkcs11:` directly
      since the key is reachable — set `GP_PKCS11_MODULE` if not using p11-kit-proxy.
- [ ] **PIN prompts:** confirm how many (per-handshake?). Consider PIN caching.

## Polish / nice-to-have

- [ ] `--cert-pin` flag (read from stdin / masked prompt) instead of `pin-value=` in URI.
- [ ] PIN caching to cut repeated prompts.
- [ ] Detect key algorithm from the cert (currently RSA assumed; ECDSA wired but
      winsign needs IEEE-P1363 → DER conversion).
- [ ] Decide: keep as a personal fork vs. propose upstream (the pkcs11 prelogin
      signer is generally useful).
