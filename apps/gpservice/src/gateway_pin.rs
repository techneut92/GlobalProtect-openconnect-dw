//! Keep the VPN gateway reachable over the physical interface across a resume.
//!
//! When openconnect connects, its vpnc-script installs a host route for the
//! gateway (`<gw> via <nexthop> dev <phys>`) so the tunnel's own control traffic
//! escapes the tunnel. On resume from sleep the physical NIC can flap, and the
//! kernel drops routes attached to an interface that went down — including that
//! host route. Without it, openconnect's reconnect/logout sockets fall back to the
//! default route, which still points at the now-dead `tun0`, and hang for the full
//! TCP timeout (~2 min) even though the physical network is already back.
//!
//! We capture that host route at connect time (when routing is healthy) and
//! re-assert it just before triggering openconnect's in-place reconnect, so its
//! control traffic reaches the portal over the physical NIC immediately — without
//! tearing the tunnel down. `tun0` stays up, so nothing else can leak in the
//! meantime (traffic to anywhere but the pinned gateway is still bound to the
//! dead tunnel = fail-closed). Routing only — no firewall.

use std::process::Command;

use log::{debug, info, warn};

/// The gateway host route captured at connect time, re-applied on reconnect.
#[derive(Clone)]
pub(crate) struct GatewayRoute {
  gw_ip: String,
  nexthop: Option<String>,
  dev: String,
}

impl GatewayRoute {
  /// Capture how the kernel currently reaches `gw_ip`, but only if that path is
  /// over a *physical* interface. Returns `None` when the lookup fails or the only
  /// route is through the tunnel (nothing useful to re-pin). Call at connect time.
  pub fn capture(gw_ip: &str) -> Option<Self> {
    let (nexthop, dev) = physical_route_to(gw_ip)?;
    info!(
      "captured gateway route: {gw_ip} via {} dev {dev}",
      nexthop.as_deref().unwrap_or("link")
    );
    Some(Self {
      gw_ip: gw_ip.to_string(),
      nexthop,
      dev,
    })
  }

  /// Re-assert the host route so the gateway is reachable over the physical NIC
  /// again. Idempotent (`ip route replace`). Returns whether it succeeded — a
  /// failure (`Network is unreachable`) means the NIC is not back yet, so this
  /// doubles as a readiness signal for the reconnect.
  pub fn reassert(&self) -> bool {
    let ok = match &self.nexthop {
      Some(nh) => ip(&["route", "replace", &self.gw_ip, "via", nh, "dev", &self.dev]),
      None => ip(&["route", "replace", &self.gw_ip, "dev", &self.dev]),
    };
    if ok {
      info!("re-pinned gateway {} to dev {} for reconnect", self.gw_ip, self.dev);
    }
    ok
  }
}

/// Ask the kernel how it reaches `gw_ip` and return `(next-hop, dev)` when that
/// path is over a physical interface.
fn physical_route_to(gw_ip: &str) -> Option<(Option<String>, String)> {
  let out = Command::new("ip").args(["route", "get", gw_ip]).output().ok()?;
  if !out.status.success() {
    return None;
  }
  let text = String::from_utf8_lossy(&out.stdout);
  // e.g. "103.56.172.6 via 10.0.0.254 dev enp6s0 src 10.0.0.50 uid 0"
  //  or  "103.56.172.6 dev enp6s0 src 10.0.0.50 uid 0" (directly attached)
  let tokens: Vec<&str> = text.split_whitespace().collect();
  let dev = tokens.windows(2).find(|w| w[0] == "dev").map(|w| w[1].to_string())?;
  if dev.starts_with("tun") || dev.starts_with("gpd") || dev.starts_with("vpn") {
    debug!("gateway {gw_ip} routes via tunnel dev {dev}; no physical route captured");
    return None;
  }
  let nexthop = tokens.windows(2).find(|w| w[0] == "via").map(|w| w[1].to_string());
  Some((nexthop, dev))
}

/// Run `ip <args>`; returns whether it succeeded. Failures are logged at debug
/// except spawn errors.
fn ip(args: &[&str]) -> bool {
  match Command::new("ip").args(args).output() {
    Ok(o) if o.status.success() => true,
    Ok(o) => {
      debug!("ip {:?} failed: {}", args, String::from_utf8_lossy(&o.stderr).trim());
      false
    }
    Err(err) => {
      warn!("failed to run `ip {:?}`: {err}", args);
      false
    }
  }
}
