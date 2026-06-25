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

/// The shell command (for display) that updates this install.
pub fn update_command(kind: InstallKind) -> Option<String> {
  match kind {
    InstallKind::Flatpak => Some(format!("flatpak update {FLATPAK_ID}")),
    // No package repo yet for the native builds — there's no upgrade command, so
    // the UI opens the release page instead.
    _ => None,
  }
}

/// Install command for the privileged backend package, per OS. Best-effort: with
/// no configured repo these fail unless the user added one / downloaded the
/// release, which is why the UI also shows instructions.
pub fn backend_install_command(kind: InstallKind) -> Option<String> {
  let pkg = "globalprotect-openconnect-dw";
  match kind {
    InstallKind::RpmOstree => Some(format!("rpm-ostree install {pkg}")),
    InstallKind::Dnf => Some(format!("dnf install {pkg}")),
    InstallKind::Apt => Some(format!("apt-get install {pkg}")),
    InstallKind::Pacman => Some(format!("pacman -S {pkg}")),
    InstallKind::Apk => Some(format!("apk add {pkg}")),
    InstallKind::Zypper => Some(format!("zypper install {pkg}")),
    InstallKind::Flatpak | InstallKind::Unknown => None,
  }
}

pub fn kind_from_str(s: &str) -> InstallKind {
  match s {
    "rpm-ostree" => InstallKind::RpmOstree,
    "dnf" => InstallKind::Dnf,
    "apt" => InstallKind::Apt,
    "pacman" => InstallKind::Pacman,
    "apk" => InstallKind::Apk,
    "zypper" => InstallKind::Zypper,
    "flatpak" => InstallKind::Flatpak,
    _ => InstallKind::Unknown,
  }
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOption {
  pub kind: String,
  pub label: String,
  pub command: Option<String>,
  pub hint: String,
}

/// All backend-install options, so the UI can offer a manual override when the
/// auto-detection is wrong. Flatpak is excluded — the backend is always a host
/// package (it needs root + host networking).
pub fn install_options() -> Vec<InstallOption> {
  [
    InstallKind::RpmOstree,
    InstallKind::Dnf,
    InstallKind::Apt,
    InstallKind::Pacman,
    InstallKind::Apk,
    InstallKind::Zypper,
  ]
  .iter()
  .map(|&k| InstallOption {
    kind: k.as_str().to_string(),
    label: kind_label(k).to_string(),
    command: backend_install_command(k),
    hint: backend_install_hint(k),
  })
  .collect()
}

/// Copy-paste shell commands to fetch and install the backend package for this
/// OS — concrete asset name + arch + version, since the fork ships via GitHub
/// releases (no repo to resolve a package name from).
pub fn backend_install_hint(kind: InstallKind) -> String {
  let v = GUI_VERSION;
  let arch = std::env::consts::ARCH; // x86_64 / aarch64
  let url = |file: &str| format!("https://github.com/{REPO}/releases/download/v{v}/{file}");
  let rpm = format!("globalprotect-openconnect-dw-{v}-1.{arch}.rpm");
  match kind {
    InstallKind::RpmOstree => {
      format!("curl -LO {}\nsudo rpm-ostree install ./{rpm}\n# then reboot", url(&rpm))
    }
    InstallKind::Dnf => format!("curl -LO {}\nsudo dnf install ./{rpm}", url(&rpm)),
    InstallKind::Apt => {
      let da = if arch == "aarch64" { "arm64" } else { "amd64" };
      let deb = format!("globalprotect-openconnect-dw_{v}-1_{da}.deb");
      format!("curl -LO {}\nsudo apt install ./{deb}", url(&deb))
    }
    InstallKind::Pacman => {
      let pkg = format!("globalprotect-openconnect-dw-{v}-1-{arch}.pkg.tar.zst");
      format!("curl -LO {}\nsudo pacman -U ./{pkg}", url(&pkg))
    }
    InstallKind::Apk => {
      let apk = format!("globalprotect-openconnect-dw-{v}-r1-{arch}.apk");
      format!("curl -LO {}\nsudo apk add --allow-untrusted ./{apk}", url(&apk))
    }
    InstallKind::Zypper => format!("curl -LO {}\nsudo zypper install ./{rpm}", url(&rpm)),
    InstallKind::Flatpak | InstallKind::Unknown => {
      format!("Download the backend package for your distro from\nhttps://github.com/{REPO}/releases/latest")
    }
  }
}

/// Full root shell script (download the release asset, then install it) for the
/// "Install" button. Works once the release is publicly downloadable; the same
/// commands are shown for copy-paste. `None` for kinds we can't script.
pub fn backend_install_script(kind: InstallKind) -> Option<String> {
  let v = GUI_VERSION;
  let arch = std::env::consts::ARCH;
  let url = |file: &str| format!("https://github.com/{REPO}/releases/download/v{v}/{file}");
  // Download into a temp dir (we run as root via pkexec), then install the file.
  let dl = |file: &str| format!("cd \"$(mktemp -d)\" && curl -fL -o '{file}' '{}'", url(file));
  let rpm = format!("globalprotect-openconnect-dw-{v}-1.{arch}.rpm");
  match kind {
    InstallKind::RpmOstree => Some(format!("{} && rpm-ostree install -y './{rpm}'", dl(&rpm))),
    InstallKind::Dnf => Some(format!("{} && dnf install -y './{rpm}'", dl(&rpm))),
    InstallKind::Apt => {
      let da = if arch == "aarch64" { "arm64" } else { "amd64" };
      let deb = format!("globalprotect-openconnect-dw_{v}-1_{da}.deb");
      Some(format!("{} && apt-get install -y './{deb}'", dl(&deb)))
    }
    InstallKind::Pacman => {
      let p = format!("globalprotect-openconnect-dw-{v}-1-{arch}.pkg.tar.zst");
      Some(format!("{} && pacman -U --noconfirm './{p}'", dl(&p)))
    }
    InstallKind::Apk => {
      let a = format!("globalprotect-openconnect-dw-{v}-r1-{arch}.apk");
      Some(format!("{} && apk add --allow-untrusted './{a}'", dl(&a)))
    }
    InstallKind::Zypper => Some(format!("{} && zypper install -y './{rpm}'", dl(&rpm))),
    InstallKind::Flatpak | InstallKind::Unknown => None,
  }
}

/// Run a root shell script via pkexec (through flatpak-spawn --host when
/// sandboxed). The script reaches the host shell as a single argument.
pub fn run_root_script(script: &str) -> bool {
  let mut cmd = if is_flatpak() {
    let mut c = Command::new("flatpak-spawn");
    c.args(["--host", "pkexec", "sh", "-c"]).arg(script);
    c
  } else {
    let mut c = Command::new("pkexec");
    c.args(["sh", "-c"]).arg(script);
    c
  };
  cmd.spawn().is_ok()
}

/// Run a privileged command on the host (via pkexec; through flatpak-spawn when
/// sandboxed). Best-effort, fire-and-forget.
pub fn run_privileged(shell_cmd: &str) -> bool {
  let parts: Vec<&str> = shell_cmd.split_whitespace().collect();
  if parts.is_empty() {
    return false;
  }
  let mut cmd = if is_flatpak() {
    let mut c = Command::new("flatpak-spawn");
    c.arg("--host").arg("pkexec").args(&parts);
    c
  } else {
    let mut c = Command::new("pkexec");
    c.args(&parts);
    c
  };
  cmd.spawn().is_ok()
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
