# Split plan — backend / GUI / protocol

End goal: **3 repositories**
1. **Backend / CLI + core** — `gpservice`, `gpclient`, `gpauth`, `gpapi`, `auth`, `openconnect`, `common`. Webkit-free.
2. **GUI** — `gpgui` (Tauri). Owns the embedded webview / SSO rendering.
3. **Protocol** — `gp-protocol`, the versioned wire contract both sides depend on.

Strategy: do the split **inside the monorepo first** (clean the seams while everything still builds together), then carve out repos once the seams are clean.

Key decisions:
- **`--browser` is the default SSO** for the CLI (webkit-free, headless-friendly).
- The **embedded webview SSO is GUI-only**, driven by request data the backend hands over **the protocol**.
- Webkit lives only in the GUI (Tauri needs it anyway); the backend has **no webview code at all** (structural, not a feature flag).
- Backend and GUI are **versioned independently**; a **`PROTOCOL_VERSION` handshake** gates compatibility (replaces the `major.minor` heuristic).

---

## Status (2026-06-27)
- v1.0.5 released — GitHub + COPR + OBS (Ubuntu 26.04) all at 1.0.5.
- Phase 1 **in progress** on branch `phase1-gp-protocol`:
  - ✅ `crates/gp-protocol` + `PROTOCOL_VERSION` (on `main`, `660a61c`)
  - ✅ `ClientOs` migrated, workspace builds, **GUI tested (connected OK)** (`4ae4a15`)
  - ✅ `gp-protocol` licensed © Dylan Westra, GPL-3.0 (`90cfa83`)
  - migration pattern proven: move type → `gpapi` re-exports → call sites unchanged → build.
- **Nothing merged to `main`** beyond the harmless `gp-protocol` crate skeleton.

## Phase 1 — Shared protocol contract (foundation)
- [x] Create `crates/gp-protocol` — single source of truth for `WsRequest` / `WsEvent` / `VpnState` / `ConnectRequest`
- [x] Add a `PROTOCOL_VERSION` constant
- [x] Migrate **all** wire types to `gp-protocol` (`gpapi::service` is now re-exports):
  `ClientOs`, `Gateway`/`PriorityRule`, `SessionInfo`/`SessionWarning` (+ time helpers),
  `ConnectInfo`/`ConnectedInfo`/`VpnState`, `ConnectArgs`/`ConnectRequest`/`DisconnectRequest`/`WsRequest`,
  `WsEvent`, `VpnEnv`. (`SessionRequestArgs`, `LaunchGuiRequest`, `UpdateGuiRequest` stay in `gpapi` — not part of the GUI↔service protocol.)
- [x] **Delete `gpgui`'s `proto.rs` mirror** → depend on `gp-protocol` (drift killed). `send_connect` is typed `ConnectRequest`; `parse_conn_details` uses typed `ConnectedInfo` via new accessors (`ConnectInfo::portal`, `ConnectedInfo::{tun_iface,ipv4,ipv6}`, `VpnState::label`). Workspace builds + GUI smoke-test.
- [x] **`PROTOCOL_VERSION` handshake** — `VpnEnv` carries the backend's `protocol_version`; the GUI checks it on connect (loopback) and refuses an incompatible protocol. (Flatpak/D-Bus still uses the package `major.minor` check — a follow-up could add a D-Bus property for parity.)
- [ ] **SSO-handoff messages** (`WsEvent::SamlAuth { url, … }` + cookie back) — **moved to Phase 3**, where the GUI's in-process webview consumes them (no point adding a message with no consumer).
- [ ] **Deferred — precise version in the mismatch message.** Today the message gives the *direction* (update / downgrade GP Client) + both protocol ranges. Telling the user a concrete "update to ≥ vX" / "downgrade to ≤ vX" needs a `protocol → app-version` map, which is manual and rots — and gets worse once the GUI/backend version independently (their version numbers stop corresponding). Clean design, do it **with the split (~Phase 3/5)**: each component embeds a small `protocol → {min,max app version}` table (filled at release time) and the backend advertises its **app version** next to its protocol range in `VpnEnv`; the GUI then computes exact guidance. Cheap interim win available anytime: for the *upgrade* case, route into the existing update-check → "update to the latest vX.Y.Z".

**Phase 1 done** (branch `phase1-gp-protocol`): protocol crate is the single source of truth, the `gpgui` mirror is gone, the version handshake is live, workspace builds, GUI connect tested.

## Phase 2 + 3 — Webkit-free backend + GUI-owned SSO (refined after the arch map)

> **Key finding:** the GUI **already owns the whole auth flow** — `gpgui` does
> prelogin + SAML SSO + builds the `ConnectRequest` itself (`connect.rs`); the
> backend (`gpservice`) only runs the tunnel. So "drive SSO over the protocol" is
> **moot** — nothing to protocol-ize. Today the GUI does the webview by **spawning
> the external `gpauth`** (`SamlAuthLauncher`, `connect.rs:178`), and that
> subprocess is the *only* thing pulling webkit into the backend (`gpclient` is
> already built `--no-default-features`; `gpservice` has no webkit).
>
> So this is **three safe, sequential steps** — the "must land together" risk is
> just ordering (**A before B**). Done on branch `phase2-webkit-free`, GUI testable
> throughout.

> **Status:** A ✅ (in-process SSO, tested — no `gpauth` spawned) and B ✅
> (`gpservice`/`gpclient`/`gpauth` = 0 webkit/tauri deps; backend package webkit
> stripped) are **done** on `phase2-webkit-free`. C (SSO caching) is next — note
> the in-process webview already gives partial caching for free (shared cookie
> store within a GUI session).

### A. GUI does SSO in-process
- [x] `apps/gpgui` depends on `auth` (`webview-auth` + `browser-auth`) directly.
- [ ] `connect.rs build_connect_request`: replace `SamlAuthLauncher…launch()` (spawn `gpauth`) with **in-process** `WebviewAuthenticator::new(server, &gp_params).with_auth_request(saml).authenticate(&app_handle)` (embedded) / `BrowserAuthenticator` (browser). Convert `SamlAuthData → Credential` via `Credential::try_from(SamlAuthResult::Success(..))`.
- [ ] **Thread the `AppHandle`** — `vpn::connect` runs in the background command-loop task (via `cmd_tx`), not the Tauri command, so stash the `AppHandle` in `AppState` at setup and read it in `build_connect_request`. ← the one tricky bit.
- [ ] Test: GUI SAML connect works in-process (no `gpauth` spawned). Nothing else breaks — `gpauth` still serves `gpclient`.

### B. Backend webkit-free (only after A works)
- [ ] `apps/gpauth`: drop `webview-auth` from `default` → browser-only (no tauri/webkit).
- [ ] `crates/auth`: keep the `webview-auth` feature but **only `gpgui` enables it**; backend uses `browser-auth`.
- [ ] `crates/gpapi`: drop the `tauri`/`gtk` feature + the empty `webview-auth` marker; `SamlAuthLauncher`'s now-unused `--default-browser` path can go.
- [ ] Strip `libwebkit2gtk` from the **backend** package (`control.in` runtime dep + `.spec` Requires); keep it on the GUI package. Verify rpm/deb smoketests + size shrink.

### C. SSO session caching (feature)
- [ ] Cache the SAML cookie/credential per identity in the keyring (`secrets.rs`). On reconnect, try it first; on portal/gateway rejection (expired) fall back to webview/browser SSO. Avoids a full re-login on every disconnect.

## Phase 4 — Independent versioning + handshake
- [ ] Give `gpgui` its own version (drop `version.workspace = true`)
- [ ] Handshake exchanges `PROTOCOL_VERSION`; mismatch → the existing update UI (replaces the `major.minor` heuristic)

## Phase 5 — Verify + release
- [ ] Backend builds with no webkit in deps; CLI `--browser` SSO works headless
- [ ] GUI (Flatpak) embedded SSO works end-to-end
- [ ] Protocol version-mismatch correctly triggers the update message
- [ ] README / changelog / CHANGES; cut a release

## Phase 6 — Later: split into 3 repos
- [ ] `gp-protocol` → own repo + tag
- [ ] backend/CLI+core repo
- [ ] GUI repo, git-deps `gpapi`/`auth` + `gp-protocol` at tags
- [ ] Per-repo CI + independent release pipelines

---

**Critical path:** Phase 1 unblocks everything (the protocol is how Phase 3 hands SSO to the GUI and how Phase 4 detects mismatch). Phases 2 and 3 land together (move the webview out, re-drive it from the GUI). Phase 6 is purely mechanical *if* 1–5 are clean — which is the whole reason to do them in the monorepo first.
