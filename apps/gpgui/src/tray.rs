//! StatusNotifierItem tray ("app indicator"). Uses the freedesktop/KDE
//! StatusNotifierItem spec, so it is native on KDE Plasma and COSMIC (both ship
//! an SNI host); on GNOME it needs the AppIndicator/AppIndicatorSupport
//! extension. When no StatusNotifierWatcher is present the tray simply fails to
//! spawn and the window still works (see `main.rs`).
//!
//! The icon encodes the connection state directly (grey/amber/green shield or
//! signal ring), in two user-selectable concepts. "Connecting" is animated by an
//! external timer (see `main.rs`) that bumps `frame` and calls `update()` — SNI
//! hosts don't play GIFs, so we swap frames.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use ksni::menu::{MenuItem, StandardItem};
use ksni::{Icon, ToolTip, Tray};
use tauri::{AppHandle, Manager};

use crate::config::Config;
use crate::state::{Shared, Status};
use crate::vpn::UiCommand;

pub type TrayHandle = ksni::blocking::Handle<GpTray>;

pub struct GpTray {
  pub shared: Arc<Mutex<Shared>>,
  pub cfg: Arc<Mutex<Config>>,
  pub cmd_tx: Sender<UiCommand>,
  /// Tauri app handle, so the tray can show the main window after close-to-tray.
  pub app: AppHandle,
  /// Current animation frame for the "connecting" state, advanced by the
  /// animator thread in `main.rs`.
  pub frame: Arc<AtomicUsize>,
}

impl GpTray {
  fn status(&self) -> Status {
    self.shared.lock().unwrap().status.clone()
  }

  fn concept(&self) -> &'static Concept {
    concept_for(&self.cfg.lock().unwrap().tray_icon)
  }

  /// Reveal (and focus) the main window after a close-to-tray.
  fn show_window(&self) {
    if let Some(w) = self.app.get_webview_window("main") {
      let _ = w.show();
      let _ = w.unminimize();
      let _ = w.set_focus();
    }
  }

  /// Full shutdown: tear down the live tunnel first (so we never leave the VPN
  /// up with no UI to manage it), then exit the process.
  fn quit(&self) {
    if self.status().is_active() {
      let _ = self.cmd_tx.send(UiCommand::Disconnect);
      for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(100));
        if !self.shared.lock().unwrap().status.is_active() {
          break;
        }
      }
    }
    std::process::exit(0);
  }

  /// Icons for the current state. `animate` picks the live connecting frame;
  /// when false (tooltip) it uses the first frame.
  fn icons(&self, animate: bool) -> Vec<Icon> {
    let c = self.concept();
    match self.status() {
      Status::Connected => decode_all(c.connected),
      // Error reuses the disconnected icon (per design — no separate error art).
      Status::Disconnected | Status::Error(_) => decode_all(c.disconnected),
      Status::Connecting | Status::Disconnecting => {
        let i = if animate { self.frame.load(Ordering::Relaxed) } else { 0 };
        let frame = c.connecting_frames[i % c.connecting_frames.len()];
        to_icon(frame).into_iter().collect()
      }
    }
  }
}

impl Tray for GpTray {
  fn id(&self) -> String {
    "gpgui-ng".into()
  }

  fn title(&self) -> String {
    "GlobalProtect".into()
  }

  // Left-click reveals the window (after close-to-tray).
  fn activate(&mut self, _x: i32, _y: i32) {
    self.show_window();
  }

  // Empty name forces SNI hosts to use our ARGB pixmap (the state icon).
  fn icon_name(&self) -> String {
    String::new()
  }

  fn icon_pixmap(&self) -> Vec<Icon> {
    self.icons(true)
  }

  fn tool_tip(&self) -> ToolTip {
    ToolTip {
      title: "GlobalProtect".into(),
      description: self.status().label(),
      icon_name: String::new(),
      icon_pixmap: self.icons(false),
    }
  }

  fn menu(&self) -> Vec<MenuItem<Self>> {
    let status = self.status();
    vec![
      StandardItem {
        label: "Open GlobalProtect".into(),
        activate: Box::new(|this: &mut Self| this.show_window()),
        ..Default::default()
      }
      .into(),
      MenuItem::Separator,
      StandardItem {
        label: format!("Status: {}", status.label()),
        enabled: false,
        ..Default::default()
      }
      .into(),
      StandardItem {
        label: "Disconnect".into(),
        enabled: status.is_active(),
        activate: Box::new(|this: &mut Self| {
          let _ = this.cmd_tx.send(UiCommand::Disconnect);
        }),
        ..Default::default()
      }
      .into(),
      MenuItem::Separator,
      StandardItem {
        label: "Quit".into(),
        activate: Box::new(|this: &mut Self| this.quit()),
        ..Default::default()
      }
      .into(),
    ]
  }
}

// ---- icon assets ----------------------------------------------------------

/// One tray concept: per-state PNGs (a couple of sizes) + connecting frames.
struct Concept {
  disconnected: &'static [&'static [u8]],
  connected: &'static [&'static [u8]],
  connecting_frames: &'static [&'static [u8]],
}

macro_rules! concept {
  ($dir:literal) => {
    Concept {
      disconnected: &[
        include_bytes!(concat!("../icons/tray/", $dir, "/color/disconnected-32.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/disconnected-64.png")),
      ],
      connected: &[
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connected-32.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connected-64.png")),
      ],
      connecting_frames: &[
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/00-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/01-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/02-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/03-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/04-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/05-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/06-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/07-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/08-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/09-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/10-64.png")),
        include_bytes!(concat!("../icons/tray/", $dir, "/color/connecting-anim/11-64.png")),
      ],
    }
  };
}

static SHIELD: Concept = concept!("shield");
static RING: Concept = concept!("ring");

fn concept_for(name: &str) -> &'static Concept {
  match name {
    "ring" => &RING,
    _ => &SHIELD,
  }
}

fn decode_all(pngs: &[&'static [u8]]) -> Vec<Icon> {
  pngs.iter().filter_map(|b| to_icon(b)).collect()
}

/// Decode an RGBA8 PNG into an ARGB32 (network byte order) SNI pixmap.
fn to_icon(bytes: &[u8]) -> Option<Icon> {
  let mut reader = png::Decoder::new(bytes).read_info().ok()?;
  let mut buf = vec![0u8; reader.output_buffer_size()];
  let info = reader.next_frame(&mut buf).ok()?;
  if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
    return None;
  }
  let rgba = &buf[..info.buffer_size()];
  let mut data = Vec::with_capacity(rgba.len());
  for px in rgba.chunks_exact(4) {
    data.push(px[3]); // A
    data.push(px[0]); // R
    data.push(px[1]); // G
    data.push(px[2]); // B
  }
  Some(Icon { width: info.width as i32, height: info.height as i32, data })
}
