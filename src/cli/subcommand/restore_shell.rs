use std::{os::unix::prelude::PermissionsExt, process::ExitCode};

use crate::{
    action::common::ConfigureShellProfile,
    cli::{ensure_root, CommandExecute},
    planner::{PlannerError, ShellProfileLocations},
};
use clap::{ArgAction, Parser};

/**
Update the macOS startup files to make Nix usable after system upgrades.
*/
#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true)]
pub struct RestoreShell {
    #[clap(
        long,
        env = "NIX_INSTALLER_NO_CONFIRM",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub no_confirm: bool,

    #[clap(
        long,
        env = "NIX_INSTALLER_EXPLAIN",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub explain: bool,
}

#[async_trait::async_trait]
impl CommandExecute for RestoreShell {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            no_confirm: _,
            explain: _,
        } = self;

        ensure_root()?;

        let mut reconfigure = ConfigureShellProfile::plan(ShellProfileLocations::default())
            .await
            .map_err(PlannerError::Action)?
            .boxed();

        if let Err(err) = reconfigure.try_execute().await {
            println!("{:#?}", err);

            /*
            let err = NixInstallerError::Action(err);
            #[cfg(feature = "diagnostics")]
            if let Some(diagnostic_data) = &self.diagnostic_data {
                diagnostic_data
                    .clone()
                    .failure(&err)
                    .send(
                        crate::diagnostics::DiagnosticAction::RestoreShell,
                        crate::diagnostics::DiagnosticStatus::Failure,
                    )
                    .await?;
            }*/
            Ok(ExitCode::FAILURE)
        } else {
            Ok(ExitCode::SUCCESS)
        }
    }
}

#[tracing::instrument(level = "debug")]
async fn copy_self_to_nix_dir() -> Result<(), std::io::Error> {
    let path = std::env::current_exe()?;
    tokio::fs::copy(path, "/nix/nix-installer").await?;
    tokio::fs::set_permissions("/nix/nix-installer", PermissionsExt::from_mode(0o0755)).await?;
    Ok(())
}
