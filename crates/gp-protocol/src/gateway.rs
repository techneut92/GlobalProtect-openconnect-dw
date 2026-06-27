use std::fmt::Display;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Serialize, Deserialize, Type, Clone)]
pub struct PriorityRule {
  pub name: String,
  pub priority: u32,
}

#[derive(Debug, Serialize, Deserialize, Type, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Gateway {
  pub name: String,
  pub address: String,
  pub priority: u32,
  pub priority_rules: Vec<PriorityRule>,
}

impl Display for Gateway {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{} ({})", self.name, self.address)
  }
}

impl Gateway {
  pub fn new(name: String, address: String) -> Self {
    Self {
      name,
      address,
      priority: 0,
      priority_rules: vec![],
    }
  }

  pub fn name(&self) -> &str {
    &self.name
  }

  pub fn server(&self) -> &str {
    &self.address
  }
}
