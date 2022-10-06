use std::{path::PathBuf, process::ExitCode};

use clap::{ArgAction, Parser};
use eyre::{eyre, WrapErr};
use harmonic::InstallPlan;

use crate::{cli::CommandExecute, interaction};

/// Execute an install using an existing plan
#[derive(Debug, Parser)]
pub(crate) struct Execute {
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    no_confirm: bool,
    #[clap(default_value = "/dev/stdin")]
    plan: PathBuf,
}

#[async_trait::async_trait]
impl CommandExecute for Execute {
    #[tracing::instrument(skip_all, fields())]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self { no_confirm, plan } = self;

        let install_plan_string = tokio::fs::read_to_string(plan)
            .await
            .wrap_err("Reading plan")?;
        let mut plan: InstallPlan = serde_json::from_str(&install_plan_string)?;

        if !no_confirm {
            if !interaction::confirm(plan.describe_execute()).await? {
                interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await;
            }
        }

        if let Err(err) = plan.install().await {
            tracing::error!("{:?}", eyre!(err));
            if !interaction::confirm(plan.describe_revert()).await? {
                interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await;
            }
            plan.revert().await?
        }

        Ok(ExitCode::SUCCESS)
    }
}
