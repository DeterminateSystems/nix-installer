use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{action::ActionState, plan::RECEIPT_LOCATION, BuiltinPlanner, InstallPlan, Planner};
use clap::{ArgAction, Parser};
use eyre::{eyre, WrapErr};

use crate::{cli::CommandExecute, interaction};

/// Execute an install (possibly using an existing plan)
///
/// To pass custom options, select a planner, for example `harmonic install linux-multi --help`
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

        let existing_receipt: Option<InstallPlan> = match Path::new(RECEIPT_LOCATION).exists() {
            true => {
                let install_plan_string = tokio::fs::read_to_string(&RECEIPT_LOCATION)
                    .await
                    .wrap_err("Reading plan")?;
                Some(serde_json::from_str(&install_plan_string)?)
            },
            false => None,
        };

        let mut install_plan = match (planner, plan) {
            (Some(planner), None) => {
                let chosen_planner: Box<dyn Planner> = planner.clone().boxed();

                match existing_receipt {
                    Some(existing_receipt) => {
                        if existing_receipt.planner.typetag_name() != chosen_planner.typetag_name() {
                            return Err(eyre!("Found existing plan in `{RECEIPT_LOCATION}` which used a different planner, try uninstalling the existing install"))
                        }
                        if existing_receipt.planner.settings().map_err(|e| eyre!(e))? != chosen_planner.settings().map_err(|e| eyre!(e))? {
                            return Err(eyre!("Found existing plan in `{RECEIPT_LOCATION}` which used different planner settings, try uninstalling the existing install"))
                        }
                        if existing_receipt.actions.iter().all(|v| v.action_state() == ActionState::Completed) {
                            return Err(eyre!("Found existing plan in `{RECEIPT_LOCATION}`, with the same settings, already completed, try uninstalling and reinstalling if Nix isn't working"))
                        }
                        existing_receipt
                    } ,
                    None => {
                        planner.plan().await.map_err(|e| eyre!(e))?
                    },
                }
            },
            (None, Some(plan_path)) => {
                let install_plan_string = tokio::fs::read_to_string(&plan_path)
                .await
                .wrap_err("Reading plan")?;
                serde_json::from_str(&install_plan_string)?
            },
            (None, None) => {
                let builtin_planner = BuiltinPlanner::default()
                    .await
                    .map_err(|e| eyre::eyre!(e))?;
                builtin_planner.plan().await.map_err(|e| eyre!(e))?
            },
            (Some(_), Some(_)) => return Err(eyre!("`--plan` conflicts with passing a planner, a planner creates plans, so passing an existing plan doesn't make sense")),
        };

        if !no_confirm {
            if !interaction::confirm(
                install_plan
                    .describe_execute(explain)
                    .map_err(|e| eyre!(e))?,
            )
            .await?
            {
                interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await;
            }
        }

        if let Err(err) = install_plan.install().await {
            let error = eyre!(err).wrap_err("Install failure");
            if !no_confirm {
                tracing::error!("{:?}", error);
                if !interaction::confirm(install_plan.describe_revert(explain)).await? {
                    interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await;
                }
                install_plan.revert().await?
            } else {
                return Err(error);
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}
