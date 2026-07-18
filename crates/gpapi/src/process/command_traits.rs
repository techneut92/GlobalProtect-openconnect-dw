use tokio::process::Command;
use uzers::os::unix::UserExt;

use super::{desktop_session_env, users::get_non_root_user};

pub trait CommandExt {
  fn into_non_root(self) -> anyhow::Result<Command>;
}

impl CommandExt for Command {
  fn into_non_root(mut self) -> anyhow::Result<Command> {
    let user = get_non_root_user().map_err(|_| anyhow::anyhow!("{:?} cannot be run as root", self))?;

    // Recover the user's desktop session env (DISPLAY, DBUS_SESSION_BUS_ADDRESS,
    // XDG_*) so processes launched from a root context (e.g. browser auth from
    // `sudo gpclient connect`) can still reach the graphical session.
    desktop_session_env::apply(&mut self, user.uid(), user.home_dir());

    self
      .env("HOME", user.home_dir())
      .env("USER", user.name())
      .env("LOGNAME", user.name())
      .env("USERNAME", user.name())
      .uid(user.uid())
      .gid(user.primary_group_id());

    Ok(self)
  }
}
