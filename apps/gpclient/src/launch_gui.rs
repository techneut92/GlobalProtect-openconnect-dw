use std::env::temp_dir;

use clap::Args;
use common::constants::GP_CALLBACK_PORT_FILENAME;
use log::info;
use tokio::io::AsyncWriteExt;

/// `launch-gui` is the registered handler for the `globalprotectcallback:`
/// URL scheme (see the gpclient.desktop entry). When the browser finishes an
/// SSO sign-in it redirects to that scheme; the desktop system invokes this
/// command with the callback URL, and we hand the data to the `gpclient
/// connect` that is waiting for it on a local port. It no longer launches any
/// GUI — the graphical client is the separate gp-client app.
#[derive(Args)]
pub(crate) struct LaunchGuiArgs {
  #[arg(
    required = false,
    help = "The globalprotectcallback: URL delivered by the browser after SSO sign-in"
  )]
  pub auth_data: Option<String>,
}

pub(crate) struct LaunchGuiHandler<'a> {
  args: &'a LaunchGuiArgs,
}

impl<'a> LaunchGuiHandler<'a> {
  pub(crate) fn new(args: &'a LaunchGuiArgs) -> Self {
    Self { args }
  }

  pub(crate) async fn handle(&self) -> anyhow::Result<()> {
    // `launch-gui` cannot be run as root (it talks to a per-user callback port).
    let user = whoami::username();
    if user == "root" {
      anyhow::bail!("`launch-gui` cannot be run as root");
    }

    let auth_data = self.args.auth_data.as_deref().unwrap_or_default();
    if auth_data.is_empty() {
      anyhow::bail!(
        "`launch-gui` handles the browser SSO callback; it does not start a session. \
         Run `gpclient connect <server>` to connect."
      );
    }

    info!("Received auth callback data");
    // Process the authentication data, its format is `globalprotectcallback:<data>`
    feed_auth_data(auth_data).await
  }
}

async fn feed_auth_data(auth_data: &str) -> anyhow::Result<()> {
  if let Err(err) = feed_auth_data_cli(auth_data).await {
    info!("Failed to feed auth data to the CLI: {}", err);
  }

  Ok(())
}

async fn feed_auth_data_cli(auth_data: &str) -> anyhow::Result<()> {
  info!("Feeding auth data to the CLI");

  let port_file = temp_dir().join(GP_CALLBACK_PORT_FILENAME);
  let port = tokio::fs::read_to_string(port_file).await?;
  let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port.trim())).await?;

  stream.write_all(auth_data.as_bytes()).await?;

  Ok(())
}
