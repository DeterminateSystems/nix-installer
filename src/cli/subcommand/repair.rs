use std::process::ExitCode;

use crate::{
    action::common::ConfigureShellProfile,
    cli::{ensure_root, CommandExecute},
    planner::{PlannerError, ShellProfileLocations},
};
use clap::{ArgAction, Parser};

/**
Update the shell profiles to make Nix usable after system upgrades.
*/
#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Repair {
    #[clap(
        long,
        env = "NIX_INSTALLER_NO_CONFIRM",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub no_confirm: bool,
}

#[async_trait::async_trait]
impl CommandExecute for Repair {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self { no_confirm: _ } = self;

        ensure_root()?;

        let mut reconfigure = ConfigureShellProfile::plan(ShellProfileLocations::default())
            .await
            .map_err(PlannerError::Action)?
            .boxed();

        if let Err(err) = reconfigure.try_execute().await {
            println!("{:#?}", err);
            return Ok(ExitCode::FAILURE)
        }

        #[cfg(target_os = "macos")]
        {
            let mut reconfigure = crate::action::macos::ConfigureRemoteBuilding::plan()
                .await
                .map_err(PlannerError::Action)?
                .boxed();

            if let Err(err) = reconfigure.try_execute().await {
                println!("{:#?}", err);
                return Ok(ExitCode::FAILURE)
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}
