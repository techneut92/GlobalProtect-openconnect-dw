# Features TODO

Deferred features / enhancements, split out from the phased work in
[`split-plan.md`](./split-plan.md).

## SSO session caching (opt-in, experimental)

Avoid a full SSO re-login on every reconnect by caching the post-SSO credential
and reusing it until it expires. `gpclient` already does this (it reuses
`AuthCookieCredential`, with a `--no-reuse` flag); this brings it to the GUI as an
opt-in toggle.

- [ ] **Settings → Authentication → "Remember SSO session (experimental)"** toggle
  (off by default): a `cache_sso` config field + the switch in `settings.html` +
  the `SettingsForm` in `main.rs`. Keep the **(experimental)** tag in the label
  until the cached-cookie reconnect behavior is field-validated.
- [ ] **Cache layer** (`secrets.rs`): store/load/clear the post-SSO credential
  **keyed by server** (avoids threading the identity name) in the keyring; gated on
  the toggle.
- [ ] **Connect logic** (`connect.rs`): if the toggle is on and a cached cred
  exists, try `gateway_login` with it and **skip the webview**; on rejection
  (expired) fall back to fresh SSO + re-cache.
- [ ] **Correctness to verify**: the durable cookie is GP's `portal_userauthcookie`;
  `gpclient` already reuses `AuthCookieCredential` this way. Needs a real
  **cached-reconnect test** to confirm the GUI path skips SSO (durable) rather than
  falling back (one-time cookie).

> Note: the in-process webview (Phase 2) already gives *partial* caching for free —
> a shared cookie store within a single running GUI session. This feature adds
> cross-restart persistence and skipping the webview entirely.
