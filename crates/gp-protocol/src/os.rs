use serde::{Deserialize, Serialize};
use specta::Type;

/// Client OS reported to the portal/gateway and carried in connect requests.
#[derive(Debug, Serialize, Deserialize, Clone, Type, Default)]
pub enum ClientOs {
  #[cfg_attr(not(target_os = "macos"), default)]
  Linux,
  Windows,
  #[cfg_attr(target_os = "macos", default)]
  Mac,
}

impl From<&str> for ClientOs {
  fn from(os: &str) -> Self {
    match os {
      "Linux" => ClientOs::Linux,
      "Windows" => ClientOs::Windows,
      "Mac" => ClientOs::Mac,
      _ => ClientOs::Linux,
    }
  }
}

impl ClientOs {
  pub fn as_str(&self) -> &str {
    match self {
      ClientOs::Linux => "Linux",
      ClientOs::Windows => "Windows",
      ClientOs::Mac => "Mac",
    }
  }

  pub fn to_openconnect_os(&self) -> &str {
    match self {
      ClientOs::Linux => "linux",
      ClientOs::Windows => "win",
      ClientOs::Mac => "mac-intel",
    }
  }
}
