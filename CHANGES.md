# Modifications

This is a **modified version** of
[GlobalProtect-openconnect](https://github.com/yuezk/GlobalProtect-openconnect)
by Kevin Yue, licensed under **GPL-3.0**.

Per GPLv3 §5(a), the changes made to the original work are documented below,
with dates.

## 2026-06-18 — Dylan Westra

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
- Added `todo.md` (test plan) and this `CHANGES.md`.

No upstream functionality was removed; the file-based `--certificate` path is
unchanged.
