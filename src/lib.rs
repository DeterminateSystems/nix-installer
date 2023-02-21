/*! The Determinate [Nix](https://github.com/NixOS/nix) Installer

`nix-installer` breaks down into three main concepts:

* [`Action`]: An executable or revertable step, possibly orcestrating sub-[`Action`]s using things
  like [`JoinSet`](tokio::task::JoinSet)s.
* [`InstallPlan`]: A set of [`Action`]s, along with some metadata, which can be carried out to
  drive an install or revert.
* [`Planner`](planner::Planner): Something which can be used to plan out an [`InstallPlan`].

It is possible to create custom [`Action`]s and [`Planner`](planner::Planner)s to suit the needs of your project, team, or organization.

In the simplest case, `nix-installer` can be asked to determine a default plan for the platform and install
it, uninstalling if anything goes wrong:

```rust,no_run
use std::error::Error;
use nix_installer::InstallPlan;

# async fn default_install() -> color_eyre::Result<()> {
let mut plan = InstallPlan::default().await?;
match plan.install(None).await {
    Ok(()) => tracing::info!("Done"),
    Err(e) => {
        match e.source() {
            Some(source) => tracing::error!("{e}: {}", source),
            None => tracing::error!("{e}"),
        };
        plan.uninstall(None).await?;
    },
};
#
# Ok(())
# }
```

Sometimes choosing a specific planner is desired:

```rust,no_run
use std::error::Error;
use nix_installer::{InstallPlan, planner::Planner};

# async fn chosen_planner_install() -> color_eyre::Result<()> {
#[cfg(target_os = "linux")]
let planner = nix_installer::planner::steam_deck::SteamDeck::default().await?;
#[cfg(target_os = "macos")]
let planner = nix_installer::planner::macos::Macos::default().await?;

// Or call `crate::planner::BuiltinPlanner::default()`
// Match on the result to customize.

// Customize any settings...

let mut plan = InstallPlan::plan(planner).await?;
match plan.install(None).await {
    Ok(()) => tracing::info!("Done"),
    Err(e) => {
        match e.source() {
            Some(source) => tracing::error!("{e}: {}", source),
            None => tracing::error!("{e}"),
        };
        plan.uninstall(None).await?;
    },
};
#
# Ok(())
# }
```

*/

pub mod action;
mod channel_value;
#[cfg(feature = "cli")]
pub mod cli;
mod error;
mod os;
mod plan;
pub mod planner;
pub mod settings;

use std::{ffi::OsStr, process::Output};

use action::Action;

pub use channel_value::ChannelValue;
pub use error::NixInstallerError;
pub use plan::InstallPlan;
use planner::BuiltinPlanner;

use tokio::process::Command;

#[tracing::instrument(level = "debug", skip_all, fields(command = %format!("{:?}", command.as_std())))]
async fn execute_command(command: &mut Command) -> Result<Output, std::io::Error> {
    let command_str = format!("{:?}", command.as_std());
    tracing::trace!("Executing `{command_str}`");
    let output = command.output().await?;
    match output.status.success() {
        true => Ok(output),
        false => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Command `{command_str}` failed{}, stderr:\n{}\n",
                if let Some(code) = output.status.code() {
                    format!(" status {code}")
                } else {
                    "".to_string()
                },
                String::from_utf8_lossy(&output.stderr)
            ),
        )),
    }
}

#[tracing::instrument(level = "debug", skip_all, fields(
    k = %k.as_ref().to_string_lossy(),
    v = %v.as_ref().to_string_lossy(),
))]
fn set_env(k: impl AsRef<OsStr>, v: impl AsRef<OsStr>) {
    tracing::trace!("Setting env");
    std::env::set_var(k.as_ref(), v.as_ref());
}
