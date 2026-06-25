//! System integration for the About / updates / install flows:
//!   - detect how the app is installed (Flatpak, rpm-ostree, dnf, apt, …) so we
//!     can phrase update / backend-install actions for the user's OS,
//!   - check the GitHub Releases API for a newer fork version,
//!   - detect whether the privileged backend (gpservice) is installed and what
//!     version it is, so we can warn on a GUI↔backend mismatch.
//!
//! The fork is distributed via GitHub Releases (no dnf/apt repo yet), so package
//! managers can't auto-upgrade from a remote — "update" is `flatpak update` on
//! Flatpak, otherwise opening the release page. Backend install is best-effort
//! via pkexec, with OS-fitting instructions as the fallback.

use std::path::Path;
use std::process::Command;

const REPO: &str = "techneut92/GlobalProtect-openconnect-dw";
const FLATPAK_ID: &str = "io.github.techneut92.gpgui";
pub const GUI_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallKind {
  Flatpak,
  RpmOstree,
  Dnf,
  Apt,
  Pacman,
  Apk,
  Zypper,
  Unknown,
}

impl InstallKind {
  fn as_str(self) -> &'static str {
    match self {
      InstallKind::Flatpak => "flatpak",
      InstallKind::RpmOstree => "rpm-ostree",
      InstallKind::Dnf => "dnf",
      InstallKind::Apt => "apt",
      InstallKind::Pacman => "pacman",
      InstallKind::Apk => "apk",
      InstallKind::Zypper => "zypper",
      InstallKind::Unknown => "unknown",
    }
  }
}

pub fn is_flatpak() -> bool {
  Path::new("/.flatpak-info").exists()
}

/// How *this* binary is running, independent of the OS package manager — so a
/// source/dev build isn't mislabelled as "rpm-ostree" just because the OS is
/// image-based. Used for the About display; install/update *commands* still use
/// `detect()` (the OS package manager).
pub fn run_mode() -> &'static str {
  // A build can bake in its kind (GP_BUILD_KIND=flatpak / native / source) so the
  // version line is unambiguous regardless of where the binary sits; otherwise
  // fall back to runtime detection.
  if let Some(k) = option_env!("GP_BUILD_KIND") {
    match k {
      "flatpak" => return "Flatpak",
      "native" => return "Native package",
      "source" => return "Source build",
      "" => {}
      other => return other,
    }
  }
  if is_flatpak() {
    return "Flatpak";
  }
  if let Ok(exe) = std::env::current_exe() {
    let p = exe.to_string_lossy();
    if p.contains("/target/debug/") || p.contains("/target/release/") {
      return "Source build";
    }
    if p.starts_with("/usr/") {
      return "Native package";
    }
    if p.starts_with("/app/") {
      return "Flatpak";
    }
  }
  "Unknown"
}

/// Detect the packaging environment, preferring the most specific match.
pub fn detect() -> InstallKind {
  if is_flatpak() {
    return InstallKind::Flatpak;
  }
  // Atomic / image-based Fedora & derivatives.
  if Path::new("/run/ostree-booted").exists() {
    return InstallKind::RpmOstree;
  }
  let os = os_release();
  let id = os.get("ID").map(String::as_str).unwrap_or("");
  let like = os.get("ID_LIKE").map(String::as_str).unwrap_or("");
  let has = |needle: &str| id == needle || like.split_whitespace().any(|w| w == needle);
  if has("fedora") || has("rhel") || has("centos") {
    InstallKind::Dnf
  } else if has("debian") || has("ubuntu") {
    InstallKind::Apt
  } else if has("arch") {
    InstallKind::Pacman
  } else if has("alpine") {
    InstallKind::Apk
  } else if has("suse") || has("opensuse") {
    InstallKind::Zypper
  } else {
    InstallKind::Unknown
  }
}

fn os_release() -> std::collections::HashMap<String, String> {
  let mut map = std::collections::HashMap::new();
  if let Ok(text) = std::fs::read_to_string("/etc/os-release") {
    for line in text.lines() {
      if let Some((k, v)) = line.split_once('=') {
        map.insert(k.trim().to_string(), v.trim().trim_matches('"').to_string());
      }
    }
  }
  map
}

pub fn os_pretty_name() -> String {
  os_release().get("PRETTY_NAME").cloned().unwrap_or_else(|| "Linux".into())
}

/// The **host** `~/.config` directory for files consumed by host services
/// (autostart entries, the Pop Shell float-rule). In Flatpak, `XDG_CONFIG_HOME`
/// points at the sandbox, so the app's own config goes there — but those two
/// files must reach the real `~/.config` (exposed via `--filesystem=xdg-config/…`).
pub fn host_config_dir() -> Option<std::path::PathBuf> {
  if is_flatpak() {
    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
  } else {
    directories::BaseDirs::new().map(|d| d.config_dir().to_path_buf())
  }
}

/// Run `<bin> --version` and pull the first `x.y.z`-looking token out of stdout.
fn binary_version(bin: &str) -> Option<String> {
  let out = Command::new(bin).arg("--version").output().ok()?;
  if !out.status.success() {
    return None;
  }
  let text = String::from_utf8_lossy(&out.stdout);
  text
    .split_whitespace()
    .find(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.contains('.'))
    .map(|t| t.trim_matches(|c: char| !c.is_ascii_digit() && c != '.').to_string())
}

/// The installed backend's version, or `None` when gpservice isn't present.
pub fn backend_version() -> Option<String> {
  let bin = crate::config::gpservice_binary();
  if !Path::new(&bin).exists() {
    return None;
  }
  binary_version(&bin)
}

pub fn backend_installed() -> bool {
  // In a Flatpak the host binary isn't visible in the sandbox, so probe the
  // backend's system D-Bus name instead (that's also how we reach it).
  if is_flatpak() {
    return backend_dbus_available();
  }
  Path::new(&crate::config::gpservice_binary()).exists()
}

/// Whether the backend's system D-Bus service is installed (activatable) or
/// already running.
fn backend_dbus_available() -> bool {
  const SVC: &str = "io.github.techneut92.GPService";
  let Ok(conn) = zbus::blocking::Connection::system() else { return false };
  let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(&conn) else { return false };
  if let Ok(names) = proxy.list_activatable_names() {
    if names.iter().any(|n| n.as_str() == SVC) {
      return true;
    }
  }
  zbus::names::BusName::try_from(SVC)
    .ok()
    .and_then(|n| proxy.name_has_owner(n).ok())
    .unwrap_or(false)
}

/// Compare dotted version strings numerically. Returns Ordering of `a` vs `b`.
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
  let parse = |s: &str| -> Vec<u64> {
    s.trim_start_matches('v')
      .split(|c: char| c == '.' || c == '-')
      .map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0))
      .collect()
  };
  let (va, vb) = (parse(a), parse(b));
  for i in 0..va.len().max(vb.len()) {
    let x = va.get(i).copied().unwrap_or(0);
    let y = vb.get(i).copied().unwrap_or(0);
    if x != y {
      return x.cmp(&y);
    }
  }
  std::cmp::Ordering::Equal
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Release {
  pub version: String,
  pub url: String,
  pub notes: String,
}

/// Fetch the latest GitHub release. Errors are surfaced as a message so the UI
/// can show "couldn't check" rather than silently failing.
pub async fn latest_release() -> Result<Release, String> {
  let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
  let client = reqwest::Client::builder()
    .user_agent(format!("gpgui/{GUI_VERSION}"))
    .build()
    .map_err(|e| e.to_string())?;
  let resp = client
    .get(&url)
    .header("Accept", "application/vnd.github+json")
    .send()
    .await
    .map_err(|_| "couldn't reach the update server".to_string())?;
  if resp.status() == reqwest::StatusCode::NOT_FOUND {
    // GitHub returns 404 for private repos to anonymous callers, and also when
    // there's no published release.
    return Err("no public release found (is the repository public?)".into());
  }
  let resp = resp.error_for_status().map_err(|_| "the update server returned an error".to_string())?;
  let json: serde_json::Value = resp.json().await.map_err(|_| "unexpected update response".to_string())?;
  Ok(Release {
    version: json["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string(),
    url: json["html_url"].as_str().unwrap_or("").to_string(),
    notes: json["body"].as_str().unwrap_or("").to_string(),
  })
}

fn kind_label(kind: InstallKind) -> &'static str {
  match kind {
    InstallKind::RpmOstree => "Atomic / image-based (rpm-ostree)",
    InstallKind::Dnf => "Fedora / RHEL (dnf)",
    InstallKind::Apt => "Debian / Ubuntu (apt)",
    InstallKind::Pacman => "Arch (pacman)",
    InstallKind::Apk => "Alpine (apk)",
    InstallKind::Zypper => "openSUSE (zypper)",
    InstallKind::Flatpak => "Flatpak",
    InstallKind::Unknown => "Other",
  }
}

fn install_note(kind: InstallKind) -> &'static str {
  match kind {
    InstallKind::RpmOstree => "Layered packages take effect after the next reboot.",
    _ => "The helper service is enabled and started automatically once installed.",
  }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
  pub label: String,
  pub cmd: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOption {
  pub kind: String,
  pub label: String,
  pub note: String,
  pub steps: Vec<Step>,
}

/// Concrete, copyable install steps per OS — real release asset name, arch and
/// version. The fork ships via GitHub releases (no repo), so dnf/pacman/zypper
/// install straight from the asset URL while rpm-ostree/apt/apk download first.
/// Flatpak is excluded — the backend is always a host package.
pub fn install_options() -> Vec<InstallOption> {
  let v = GUI_VERSION;
  let arch = std::env::consts::ARCH; // x86_64 / aarch64
  let deb_arch = if arch == "aarch64" { "arm64" } else { "amd64" };
  let base = format!("https://github.com/{REPO}/releases/download/v{v}");
  let rpm = format!("globalprotect-openconnect-dw-{v}-1.{arch}.rpm");
  let deb = format!("globalprotect-openconnect-dw_{v}-1_{deb_arch}.deb");
  let pac = format!("globalprotect-openconnect-dw-{v}-1-{arch}.pkg.tar.zst");
  let apk = format!("globalprotect-openconnect-dw-{v}-r1-{arch}.apk");

  let opt = |kind: InstallKind, steps: Vec<(&str, String)>| InstallOption {
    kind: kind.as_str().to_string(),
    label: kind_label(kind).to_string(),
    note: install_note(kind).to_string(),
    steps: steps.into_iter().map(|(l, c)| Step { label: l.to_string(), cmd: c }).collect(),
  };

  vec![
    opt(InstallKind::RpmOstree, vec![
      ("Download the package", format!("curl -LO {base}/{rpm}")),
      ("Layer it onto the system", format!("sudo rpm-ostree install ./{rpm}")),
      ("Reboot to finish", "systemctl reboot".to_string()),
    ]),
    opt(InstallKind::Dnf, vec![("Install from the release", format!("sudo dnf install {base}/{rpm}"))]),
    opt(InstallKind::Apt, vec![
      ("Download the package", format!("curl -LO {base}/{deb}")),
      ("Install it", format!("sudo apt install ./{deb}")),
    ]),
    opt(InstallKind::Pacman, vec![("Install the package", format!("sudo pacman -U {base}/{pac}"))]),
    opt(InstallKind::Zypper, vec![("Install from the release", format!("sudo zypper install {base}/{rpm}"))]),
    opt(InstallKind::Apk, vec![
      ("Download the package", format!("curl -LO {base}/{apk}")),
      ("Install it", format!("sudo apk add --allow-untrusted ./{apk}")),
    ]),
  ]
}

/// Run `flatpak update` for the GUI (only meaningful on Flatpak installs).
pub fn run_flatpak_update() -> bool {
  if !is_flatpak() {
    return false;
  }
  Command::new("flatpak-spawn")
    .arg("--host")
    .arg("flatpak")
    .arg("update")
    .arg("-y")
    .arg(FLATPAK_ID)
    .spawn()
    .is_ok()
}

/// Open an http(s) URL in the host browser.
pub fn open_url(url: &str) {
  if !(url.starts_with("https://") || url.starts_with("http://")) {
    return;
  }
  if is_flatpak() {
    let _ = Command::new("flatpak-spawn").arg("--host").arg("xdg-open").arg(url).spawn();
  } else {
    let _ = Command::new("xdg-open").arg(url).spawn();
  }
}

/// `InstallKind` as a UI-friendly string.
pub fn install_kind_str(kind: InstallKind) -> &'static str {
  kind.as_str()
}
