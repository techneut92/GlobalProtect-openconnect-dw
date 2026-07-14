mod auth_flow;
mod cli;
mod dbus_service;
mod gateway_pin;
mod sleep_monitor;
mod vpn_task;

#[tokio::main]
async fn main() {
  cli::run().await;
}
