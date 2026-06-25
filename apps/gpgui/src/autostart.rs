//! XDG autostart ("Run at system startup"). Writes/removes a desktop entry in
//! `~/.config/autostart/`. The entry launches the GUI with `--hidden` so it
//! starts straight to the tray instead of popping the window at login.

use std::path::PathBuf;

const ENTRY_NAME: &str = "gpgui.desktop";
/// Installed launcher; preferred so the autostart entry survives `cargo` rebuilds.
const INSTALLED_BIN: &str = "/usr/bin/gpgui";

fn autostart_path() -> Option<PathBuf> {
  directories::BaseDirs::new().map(|d| d.config_dir().join("autostart").join(ENTRY_NAME))
}

/// The command the autostart entry runs. Uses the installed binary if present,
/// else the current executable (dev builds).
fn exec_line() -> String {
  let bin = if std::path::Path::new(INSTALLED_BIN).exists() {
    INSTALLED_BIN.to_string()
  } else {
    std::env::current_exe()
      .ok()
      .and_then(|p| p.to_str().map(str::to_string))
      .unwrap_or_else(|| "gpgui".to_string())
  };
  // WEBKIT_DISABLE_DMABUF_RENDERER mirrors the .desktop launcher; --hidden starts
  // minimized to the tray.
  format!("env WEBKIT_DISABLE_DMABUF_RENDERER=1 {bin} --hidden")
}

/// Create or remove the autostart entry to match `enabled`.
pub fn set(enabled: bool) {
  let Some(path) = autostart_path() else { return };
  if enabled {
    if let Some(dir) = path.parent() {
      let _ = std::fs::create_dir_all(dir);
    }
    let entry = format!(
      "[Desktop Entry]\n\
       Type=Application\n\
       Name=GlobalProtect\n\
       Comment=Connect to GlobalProtect VPN\n\
       Exec={}\n\
       Icon=gpgui\n\
       Terminal=false\n\
       Categories=Network;Security;\n\
       X-GNOME-Autostart-enabled=true\n",
      exec_line()
    );
    let _ = std::fs::write(&path, entry);
  } else {
    let _ = std::fs::remove_file(&path);
  }
}
