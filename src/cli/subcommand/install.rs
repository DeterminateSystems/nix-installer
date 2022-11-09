use std::{path::PathBuf, process::ExitCode};

use crate::BuiltinPlanner;
use clap::{ArgAction, Parser};
use eyre::{eyre, WrapErr};

use crate::{cli::CommandExecute, interaction};

/// Execute an install (possibly using an existing plan)
#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Install {
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub no_confirm: bool,

    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub explain: bool,
    #[clap(env = "HARMONIC_PLAN")]
    pub plan: Option<PathBuf>,

    #[clap(subcommand)]
    pub planner: Option<BuiltinPlanner>,
}

#[async_trait::async_trait]
impl CommandExecute for Install {
    #[tracing::instrument(skip_all, fields())]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            no_confirm,
            plan,
            planner,
            explain,
        } = self;

        let planner = match planner {
            Some(planner) => planner,
            None => BuiltinPlanner::default()
                .await
                .map_err(|e| eyre::eyre!(e))?,
        };

        let mut plan = match &plan {
            Some(plan_path) => {
                let install_plan_string = tokio::fs::read_to_string(&plan_path)
                    .await
                    .wrap_err("Reading plan")?;
                serde_json::from_str(&install_plan_string)?
            },
            None => planner.plan().await.map_err(|e| eyre!(e))?,
        };

        if !no_confirm {
            if !interaction::confirm(plan.describe_execute(explain).map_err(|e| eyre!(e))?).await? {
                interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await;
            }
        }

        if let Err(err) = plan.install().await {
            tracing::error!("{:?}", eyre!(err));
            if !interaction::confirm(plan.describe_revert(explain)).await? {
                interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await;
            }
            plan.revert().await?
        }

        Ok(ExitCode::SUCCESS)
    }
}
