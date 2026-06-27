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
- [ ] Migrate remaining wire types (each: move → re-export from `gpapi` → build). Per-type care: field visibility / where impls live.
  - [x] `ClientOs`
  - [ ] `Gateway` (+ `PriorityRule`) — fields are `pub(crate)`, `parse_gateways` sets them directly → add a constructor
  - [ ] `SessionInfo` / `SessionWarning`
  - [ ] `ConnectInfo` / `ConnectedInfo` / `VpnState`
  - [ ] `ConnectArgs` / `ConnectRequest` / `DisconnectRequest` / `WsRequest`
  - [ ] `WsEvent`
  - [ ] `VpnEnv`
- [ ] Delete `gpgui`'s hand-mirrored `proto.rs`; depend on `gp-protocol` instead (kills the drift)
- [ ] Add protocol messages that hand SSO to the GUI: e.g. `WsEvent::SamlAuth { url, … }` (backend → GUI "start embedded flow with this data") + the cookie coming back

## Phase 2 — Webkit-free backend (extract the webview)

> ⚠️ **Phases 2 and 3 must land together.** Today the GUI does SSO by spawning the
> webview `gpauth` (`SamlAuthLauncher.auth_executable`). Removing the webview from
> the backend breaks that path unless the GUI's own in-process webview SSO
> (Phase 3) lands in the same change. Don't merge a half — the intermediate state
> has no working SSO. Best done with the GUI runnable to test (not unattended).

- [ ] Move `crates/auth/src/webview/webview_auth.rs` + its `tauri`/webkit deps into `apps/gpgui`
- [ ] Delete the `webview-auth` feature from `gpapi` / `auth` / `gpauth` / `gpclient` (structural, not gated)
- [ ] Make `--browser` the default SSO path for the CLI
- [ ] Decide `gpauth`: keep as a webkit-free browser-only helper, or fold into `gpclient`
- [ ] Strip `webkit2gtk` / `libsoup` / `gtk` / appindicator from backend packaging (`control.in`, `.spec`) → leaner deps, more buildable distros
- [ ] Re-verify packaging builds shrink + still install (rpm/deb smoketests already gate this)

## Phase 3 — GUI embedded SSO, protocol-driven
- [ ] GUI runs the embedded SAML in its own Tauri webview (no spawned `gpauth`; drop `auth_executable`)
- [ ] Wire the flow: backend prelogin → `SamlAuth` over the protocol → GUI opens webview → captures cookie → `ConnectRequest` back

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
