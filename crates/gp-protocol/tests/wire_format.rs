//! Wire-format guard.
//!
//! Serializes a fully-populated sample of every top-level protocol message to
//! JSON and compares it against a committed golden snapshot. If the wire format
//! changes, this test fails — forcing a deliberate choice:
//!
//!   * intentional protocol change → bump `PROTOCOL_MAX` in `src/lib.rs` and
//!     regenerate the snapshot:
//!       `UPDATE_WIRE_SNAPSHOT=1 cargo test -p gp-protocol --test wire_format`
//!   * unintentional → revert the change to the wire types.
//!
//! This is the automation that keeps `PROTOCOL_MAX` honest: the wire can't drift
//! without someone seeing this diff.

use gp_protocol::request::UpdateLogLevelRequest;
use gp_protocol::*;

fn sample_gateway() -> Gateway {
  Gateway {
    name: "Gateway-1".to_string(),
    address: "vpn.example.com".to_string(),
    priority: 5,
    priority_rules: vec![PriorityRule {
      name: "rule".to_string(),
      priority: 1,
    }],
  }
}

fn sample_connect_info() -> ConnectInfo {
  ConnectInfo::new(
    "portal.example.com".to_string(),
    sample_gateway(),
    vec![sample_gateway()],
  )
}

fn sample_session_info() -> SessionInfo {
  SessionInfo {
    lifetime_secs: Some(43_200),
    user_expires: Some(1_900_000_000),
    expires_in_human: Some("12h".to_string()),
    lifetime_warning: Some(SessionWarning {
      prior_secs: 1_800,
      message: "Session expires soon".to_string(),
    }),
    inactivity_warning: Some(SessionWarning {
      prior_secs: 600,
      message: "Idle".to_string(),
    }),
    admin_logout_message: Some("Logged out by admin".to_string()),
    allow_extend_session: true,
  }
}

fn sample_connected_info() -> ConnectedInfo {
  ConnectedInfo::new(sample_connect_info(), Some(sample_session_info())).with_tunnel(
    Some("tun0".to_string()),
    Some("10.0.0.2".to_string()),
    Some("fd00::2".to_string()),
  )
}

fn sample_connect_request() -> ConnectRequest {
  ConnectRequest::new(sample_connect_info(), "authcookie=AUTH".to_string())
    .with_vpnc_script("/etc/vpnc-script".to_string())
    .with_hip(true)
    .with_csd_uid(1000)
    .with_csd_wrapper("/usr/bin/hipreport.sh".to_string())
    .with_user_agent("PAN GlobalProtect".to_string())
    .with_os(ClientOs::Linux)
    .with_os_version("Linux 6.0".to_string())
    .with_client_version("6.0.0")
    .with_certificate("/path/cert.pem".to_string())
    .with_sslkey("/path/key.pem".to_string())
    .with_key_password("secret".to_string())
    .with_reconnect_timeout(300)
    .with_mtu(1400)
    .with_disable_ipv6(true)
    .with_no_dtls(true)
    .with_local_hostname("host".to_string())
    .with_force_dpd(30)
    .with_no_xmlpost(true)
    .with_allow_extend_session(true)
}

fn sample_vpn_env() -> VpnEnv {
  VpnEnv {
    protocol_min: PROTOCOL_MIN,
    protocol_max: PROTOCOL_MAX,
    vpn_state: VpnState::Connected(Box::new(sample_connected_info())),
    vpnc_script: Some("/etc/vpnc-script".to_string()),
    csd_wrapper: Some("/usr/bin/hipreport.sh".to_string()),
    auth_executable: "/usr/bin/gpauth".to_string(),
  }
}

fn json<T: serde::Serialize>(value: &T) -> String {
  serde_json::to_string_pretty(value).expect("serialize")
}

fn render_snapshot() -> String {
  let samples: Vec<(&str, String)> = vec![
    (
      "WsRequest::Connect",
      json(&WsRequest::Connect(Box::new(sample_connect_request()))),
    ),
    ("WsRequest::Disconnect", json(&WsRequest::Disconnect(DisconnectRequest))),
    (
      "WsRequest::UpdateLogLevel",
      json(&WsRequest::UpdateLogLevel(UpdateLogLevelRequest("debug".to_string()))),
    ),
    ("WsEvent::VpnEnv", json(&WsEvent::VpnEnv(sample_vpn_env()))),
    (
      "WsEvent::VpnState(Connecting)",
      json(&WsEvent::VpnState(VpnState::Connecting(Box::new(sample_connect_info())))),
    ),
    (
      "WsEvent::VpnState(Disconnected)",
      json(&WsEvent::VpnState(VpnState::Disconnected)),
    ),
    ("WsEvent::ActiveGui", json(&WsEvent::ActiveGui)),
    ("WsEvent::ResumeConnection", json(&WsEvent::ResumeConnection)),
  ];

  let mut out = format!("# gp-protocol wire format — protocol {PROTOCOL_MIN}..={PROTOCOL_MAX}\n");
  out.push_str("# Generated: UPDATE_WIRE_SNAPSHOT=1 cargo test -p gp-protocol --test wire_format\n");
  for (name, body) in &samples {
    out.push_str(&format!("\n## {name}\n{body}\n"));
  }
  out
}

#[test]
fn wire_format_is_unchanged() {
  let snapshot_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/wire_format.snapshot");
  let actual = render_snapshot();

  if std::env::var("UPDATE_WIRE_SNAPSHOT").is_ok() {
    std::fs::write(snapshot_path, &actual).expect("write snapshot");
    eprintln!("wrote {snapshot_path}");
    return;
  }

  let expected = std::fs::read_to_string(snapshot_path)
    .expect("missing tests/wire_format.snapshot — run with UPDATE_WIRE_SNAPSHOT=1 to create it");

  assert!(
    actual == expected,
    "\n\n*** The GUI<->backend wire format changed. ***\n\
     If this is an INTENTIONAL protocol change: bump PROTOCOL_MAX in \
     crates/gp-protocol/src/lib.rs, then regenerate the snapshot:\n\
     \x20   UPDATE_WIRE_SNAPSHOT=1 cargo test -p gp-protocol --test wire_format\n\
     If it was unintentional, revert the change to the wire types.\n"
  );
}
