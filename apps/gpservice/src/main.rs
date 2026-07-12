mod auth_flow;
mod cli;
mod dbus_service;
mod handlers;
mod routes;
mod sleep_monitor;
mod vpn_task;
mod ws_connection;
mod ws_server;

#[tokio::main]
async fn main() {
  cli::run().await;
}
