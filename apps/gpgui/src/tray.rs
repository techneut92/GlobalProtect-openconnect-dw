//! StatusNotifierItem tray ("app indicator"). Native on KDE; on GNOME it needs
//! the AppIndicator/AppIndicatorSupport extension.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use ksni::menu::{MenuItem, StandardItem};
use ksni::{ToolTip, Tray};

use crate::state::{Shared, Status};
use crate::vpn::UiCommand;

pub type TrayHandle = ksni::blocking::Handle<GpTray>;

pub struct GpTray {
  pub shared: Arc<Mutex<Shared>>,
  pub cmd_tx: Sender<UiCommand>,
}

impl GpTray {
  fn status(&self) -> Status {
    self.shared.lock().unwrap().status.clone()
  }
}

impl Tray for GpTray {
  fn id(&self) -> String {
    "gpgui-ng".into()
  }

  fn title(&self) -> String {
    "GlobalProtect".into()
  }

  fn icon_name(&self) -> String {
    match self.status() {
      Status::Connected => "network-vpn".into(),
      Status::Connecting | Status::Disconnecting => "network-idle".into(),
      Status::Disconnected => "network-offline".into(),
      Status::Error(_) => "network-error".into(),
    }
  }

  fn tool_tip(&self) -> ToolTip {
    ToolTip {
      title: "GlobalProtect".into(),
      description: self.status().label(),
      icon_name: self.icon_name(),
      icon_pixmap: Vec::new(),
    }
  }

  fn menu(&self) -> Vec<MenuItem<Self>> {
    let status = self.status();
    vec![
      StandardItem {
        label: format!("Status: {}", status.label()),
        enabled: false,
        ..Default::default()
      }
      .into(),
      MenuItem::Separator,
      StandardItem {
        label: "Disconnect".into(),
        icon_name: "network-offline".into(),
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
        icon_name: "application-exit".into(),
        activate: Box::new(|_: &mut Self| std::process::exit(0)),
        ..Default::default()
      }
      .into(),
    ]
  }
}
