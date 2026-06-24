#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! gpgui — Tauri front-end for GlobalProtect-openconnect (PKCS#11 fork).
//!
//! The privileged tunnel runs in `gpservice`; this GUI is unprivileged. It
//! authenticates (prelogin + SAML, via gpauth or the browser), then drives
//! gpservice over an encrypted loopback WebSocket.
//!
//! Transport seam for Flatpak: today the GUI launches gpservice via pkexec and
//! talks to it over loopback (`client` + `vpn::ensure_service`). A Flatpak build
//! can't pkexec or see `/var/run`, so that pair is the single place a future
//! D-Bus system-service transport slots in — nothing above it changes.

mod client;
mod config;
mod connect;
mod crypto;
mod dbus_client;
mod pkcs11;
mod proto;
mod state;
mod transport;
mod tray;
mod vault;
mod vpn;

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use config::Config;
use state::{Shared, Status};
use tray::GpTray;
use vault::{Identity, Vault};
use vpn::{ConnectParams, Notifier, UiCommand};

/// Shared handles exposed to the Tauri commands.
struct AppState {
  cmd_tx: std::sync::mpsc::Sender<UiCommand>,
  shared: Arc<Mutex<Shared>>,
  cfg: Arc<Mutex<Config>>,
  vault: Arc<Mutex<Vault>>,
}

/// Advanced options edited in the settings window (persisted; read at connect).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsForm {
  os: String,
  user_agent: String,
  auth_view: String,
  mtu: u32,
  reconnect_timeout: u32,
  force_dpd: u32,
  disable_ipv6: bool,
  no_dtls: bool,
  no_xmlpost: bool,
  ignore_tls_errors: bool,
  vpnc_script: String,
  local_hostname: String,
  os_version: String,
  client_version: String,
}

/// A certificate row for the picker.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CertDto {
  id: String,
  manufacturer: String,
  label: String,
  subject: String,
  cn: String,
  token: String,
  model: String,
  slot: String,
  expiry: String,
  display: String,
  uri: String,
}

/// Connection state pushed to the webview (also returned by `get_state`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatePayload {
  status: String,
  kind: i32,
  log: String,
  active: bool,
  busy: bool,
  portal: String,
  gateway: String,
  ip: String,
  iface: String,
  expires: String,
}

fn status_kind(status: &Status) -> i32 {
  match status {
    Status::Disconnected => 0,
    Status::Connecting | Status::Disconnecting => 1,
    Status::Connected => 2,
    Status::Error(_) => 3,
  }
}

fn build_state(shared: &Arc<Mutex<Shared>>) -> StatePayload {
  let s = shared.lock().unwrap();
  StatePayload {
    status: s.status.label(),
    kind: status_kind(&s.status),
    log: s.log.clone(),
    active: s.status.is_active(),
    busy: matches!(s.status, Status::Connecting | Status::Disconnecting),
    portal: s.conn.portal.clone(),
    gateway: s.conn.gateway.clone(),
    ip: s.conn.ip.clone(),
    iface: s.conn.iface.clone(),
    expires: s.conn.expires.clone(),
  }
}

// ---- commands ----

#[tauri::command]
fn get_config(state: State<AppState>) -> Config {
  state.cfg.lock().unwrap().clone()
}

#[tauri::command]
fn get_state(state: State<AppState>) -> StatePayload {
  build_state(&state.shared)
}

#[tauri::command]
fn available_modules() -> Vec<String> {
  pkcs11::available_modules()
}

#[tauri::command]
fn scan_certs(module: String) -> Vec<CertDto> {
  pkcs11::enumerate(&module)
    .unwrap_or_default()
    .into_iter()
    .map(|c| CertDto {
      display: c.display(),
      uri: c.uri(),
      cn: c.cn(),
      id: c.id,
      manufacturer: c.manufacturer,
      label: c.label,
      subject: c.subject,
      token: c.token,
      model: c.model,
      slot: c.slot,
      expiry: c.expiry,
    })
    .collect()
}

#[tauri::command]
fn browse_file(title: String) -> Option<String> {
  // Runs on a Tauri worker thread, so blocking on the portal dialog is fine.
  pollster::block_on(rfd::AsyncFileDialog::new().set_title(&title).pick_file())
    .map(|f| f.path().to_string_lossy().to_string())
}

#[tauri::command]
fn disconnect(state: State<AppState>) {
  let _ = state.cmd_tx.send(UiCommand::Disconnect);
}

/// Open (or focus) the separate Advanced settings window.
#[tauri::command]
fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
  if let Some(w) = app.get_webview_window("settings") {
    let _ = w.set_focus();
    return Ok(());
  }
  tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App("settings.html".into()))
    .title("Advanced settings")
    .inner_size(560.0, 620.0)
    .min_inner_size(560.0, 620.0)
    .resizable(false)
    .decorations(false)
    .build()
    .map_err(|e| e.to_string())?;
  Ok(())
}

/// Persist the advanced options edited in the settings window.
#[tauri::command]
fn save_settings(app: tauri::AppHandle, state: State<AppState>, form: SettingsForm) {
  {
    let mut c = state.cfg.lock().unwrap();
    c.os = form.os;
    c.user_agent = form.user_agent;
    c.auth_view = form.auth_view;
    c.mtu = form.mtu;
    c.reconnect_timeout = form.reconnect_timeout;
    c.force_dpd = form.force_dpd;
    c.disable_ipv6 = form.disable_ipv6;
    c.no_dtls = form.no_dtls;
    c.no_xmlpost = form.no_xmlpost;
    c.ignore_tls_errors = form.ignore_tls_errors;
    c.vpnc_script = form.vpnc_script;
    c.local_hostname = form.local_hostname;
    c.os_version = form.os_version;
    c.client_version = form.client_version;
    c.save();
  }
  // The main window rescans (module/cert) and uses these at connect time.
  let _ = app.emit("config-changed", ());
}

/// Connect using a saved identity (from the unlocked vault). `portal` overrides
/// the identity's portal when non-empty.
#[tauri::command]
fn connect(state: State<AppState>, identity: String, portal: String) -> Result<(), String> {
  let id = {
    let v = state.vault.lock().unwrap();
    if !v.unlocked {
      return Err("vault is locked".into());
    }
    v.identities()
      .iter()
      .find(|i| i.name == identity)
      .cloned()
      .ok_or_else(|| format!("no identity '{identity}'"))?
  };

  let server = if portal.trim().is_empty() {
    id.portal.clone()
  } else {
    portal.trim().to_string()
  };

  // os / user-agent / SSO method / CLI options come from the settings window.
  let (os, user_agent, use_browser, opts) = {
    let c = state.cfg.lock().unwrap();
    let opts = connect::ConnOpts {
      mtu: c.mtu,
      reconnect_timeout: c.reconnect_timeout,
      force_dpd: c.force_dpd,
      disable_ipv6: c.disable_ipv6,
      no_dtls: c.no_dtls,
      no_xmlpost: c.no_xmlpost,
      ignore_tls_errors: c.ignore_tls_errors,
      vpnc_script: c.vpnc_script.clone(),
      local_hostname: c.local_hostname.clone(),
      os_version: c.os_version.clone(),
      client_version: c.client_version.clone(),
    };
    (c.os.clone(), c.user_agent.clone(), c.auth_view == "browser", opts)
  };

  let cert_kind = match id.auth_method {
    0 => 1,
    1 => 2,
    _ => 0,
  };
  let cert_uri = if id.cert_id.is_empty() {
    String::new()
  } else {
    format!("pkcs11:manufacturer={};id=%{};type=cert", id.cert_manufacturer, id.cert_id)
  };

  let params = ConnectParams {
    url: server,
    as_gateway: id.as_gateway,
    os,
    user_agent,
    module_path: id.module_path,
    cert_kind,
    use_browser,
    cert_uri,
    pin: id.pin,
    cert_file: id.cert_file,
    key_file: id.key_file,
    key_password: id.key_password,
    username: id.username,
    password: id.password,
    opts,
  };
  state.cmd_tx.send(UiCommand::Connect(params)).map_err(|e| e.to_string())
}

/// Auto-detect probe form.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeForm {
  portal: String,
  cert_kind: i32,
  cert_uri: String,
  pin: String,
  cert_file: String,
  key_file: String,
  key_password: String,
  module_path: String,
}

/// Probe the portal's prelogin to discover the required auth method.
#[tauri::command]
async fn probe_auth(state: State<'_, AppState>, form: ProbeForm) -> Result<connect::ProbeResult, String> {
  let (os, user_agent) = {
    let c = state.cfg.lock().unwrap();
    (c.os.clone(), c.user_agent.clone())
  };
  let (certificate, sslkey, key_password) = match form.cert_kind {
    1 => {
      if !form.module_path.is_empty() {
        // SAFETY: set before the probe; the GUI is single-connection.
        unsafe { std::env::set_var("GP_PKCS11_MODULE", &form.module_path) };
      }
      let cert = if form.pin.is_empty() {
        form.cert_uri.clone()
      } else {
        format!("{}?pin-value={}", form.cert_uri, form.pin)
      };
      (Some(cert), None, None)
    }
    2 => (
      Some(form.cert_file.clone()),
      (!form.key_file.is_empty()).then(|| form.key_file.clone()),
      (!form.key_password.is_empty()).then(|| form.key_password.clone()),
    ),
    _ => (None, None, None),
  };
  Ok(connect::probe(form.portal.trim(), &os, &user_agent, certificate, sslkey, key_password, false).await)
}

#[tauri::command]
fn vault_status(state: State<AppState>) -> serde_json::Value {
  let v = state.vault.lock().unwrap();
  serde_json::json!({ "exists": v.exists, "unlocked": v.unlocked })
}

#[tauri::command]
fn set_master_pin(state: State<AppState>, pin: String) -> Result<(), String> {
  state.vault.lock().unwrap().set_master_pin(&pin).map_err(|e| e.to_string())
}

#[tauri::command]
fn unlock_vault(state: State<AppState>, pin: String) -> Result<(), String> {
  state.vault.lock().unwrap().unlock(&pin).map_err(|e| e.to_string())
}

#[tauri::command]
fn lock_vault(state: State<AppState>) {
  state.vault.lock().unwrap().lock();
}

#[tauri::command]
fn list_identities(state: State<AppState>) -> Result<Vec<Identity>, String> {
  let v = state.vault.lock().unwrap();
  if !v.unlocked {
    return Err("vault is locked".into());
  }
  Ok(v.identities().to_vec())
}

#[tauri::command]
fn save_identity(app: tauri::AppHandle, state: State<AppState>, identity: Identity) -> Result<(), String> {
  state.vault.lock().unwrap().upsert(identity).map_err(|e| e.to_string())?;
  let _ = app.emit("identities-changed", ());
  Ok(())
}

#[tauri::command]
fn delete_identity(app: tauri::AppHandle, state: State<AppState>, name: String) -> Result<(), String> {
  state.vault.lock().unwrap().remove(&name).map_err(|e| e.to_string())?;
  let _ = app.emit("identities-changed", ());
  Ok(())
}

/// Open (or focus) the Identities manager window.
#[tauri::command]
fn open_manager(app: tauri::AppHandle) -> Result<(), String> {
  if let Some(w) = app.get_webview_window("manager") {
    let _ = w.set_focus();
    return Ok(());
  }
  tauri::WebviewWindowBuilder::new(&app, "manager", tauri::WebviewUrl::App("manager.html".into()))
    .title("Identities")
    .inner_size(720.0, 620.0)
    .min_inner_size(720.0, 620.0)
    .resizable(false)
    .decorations(false)
    .build()
    .map_err(|e| e.to_string())?;
  Ok(())
}

fn main() {
  tracing_subscriber::fmt()
    .with_env_filter(
      tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("gpgui=info")),
    )
    .init();

  let cfg = Arc::new(Mutex::new(Config::load()));
  let shared = Arc::new(Mutex::new(Shared::default()));
  let vault_path = config::vault_path().unwrap_or_else(|| std::path::PathBuf::from("identities.enc"));
  let vault = Arc::new(Mutex::new(Vault::load(vault_path)));
  let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<UiCommand>();

  let app_state = AppState {
    cmd_tx: cmd_tx.clone(),
    shared: shared.clone(),
    cfg: cfg.clone(),
    vault: vault.clone(),
  };

  // Moved into setup (which owns the receiver and the AppHandle).
  let setup_shared = shared.clone();
  let setup_cmd_tx = cmd_tx.clone();
  let cmd_rx = Mutex::new(Some(cmd_rx));

  tauri::Builder::default()
    .manage(app_state)
    .invoke_handler(tauri::generate_handler![
      get_config,
      get_state,
      available_modules,
      scan_certs,
      browse_file,
      connect,
      disconnect,
      open_settings,
      save_settings,
      probe_auth,
      vault_status,
      set_master_pin,
      unlock_vault,
      lock_vault,
      list_identities,
      save_identity,
      delete_identity,
      open_manager
    ])
    .setup(move |app| {
      let handle = app.handle().clone();

      // Tray (optional — no StatusNotifierWatcher on bare GNOME).
      let tray = GpTray {
        shared: setup_shared.clone(),
        cmd_tx: setup_cmd_tx.clone(),
      };
      let tray_handle = match ksni::blocking::TrayMethods::spawn(tray) {
        Ok(h) => Some(Arc::new(h)),
        Err(e) => {
          tracing::warn!("tray unavailable (install/enable AppIndicator on GNOME): {e}");
          None
        }
      };

      // UI-agnostic change hook → emit the current state to the webview.
      let on_change: Arc<dyn Fn() + Send + Sync> = {
        let handle = handle.clone();
        let shared = setup_shared.clone();
        Arc::new(move || {
          let _ = handle.emit("state", build_state(&shared));
        })
      };

      let notifier = Notifier::new(setup_shared.clone(), tray_handle, on_change);
      let rx = cmd_rx.lock().unwrap().take().expect("setup runs once");
      std::thread::spawn(move || vpn::run(rx, notifier));

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running gpgui");
}
