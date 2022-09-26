use std::{process::ExitCode, path::PathBuf};

use clap::{ArgAction, Parser};
use harmonic::InstallPlan;
use eyre::WrapErr;

use crate::{
    cli::CommandExecute,
    interaction,
};

/// An opinionated, experimental Nix installer
#[derive(Debug, Parser)]
pub(crate) struct Revert {
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    no_confirm: bool,
    #[clap(default_value = "/nix/receipt.json")]
    receipt: PathBuf,
}

#[async_trait::async_trait]
impl CommandExecute for Revert {
    #[tracing::instrument(skip_all, fields())]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self { no_confirm, receipt } = self;

        let install_receipt_string = tokio::fs::read_to_string(receipt).await.wrap_err("Reading receipt")?;
        let mut plan: InstallPlan = serde_json::from_str(&install_receipt_string)?;

        if !no_confirm {
            if !interaction::confirm(plan.description()).await? {
                interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await;
            }
        }

        plan.revert().await?;

        Ok(ExitCode::SUCCESS)
    }
}
