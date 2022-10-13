use std::{path::PathBuf, process::ExitCode};

use clap::{ArgAction, Parser};
use eyre::{eyre, WrapErr};
use harmonic::{InstallPlan, InstallSettings};

use crate::{
    cli::{
        arg::{ChannelValue, PlanOptions},
        CommandExecute,
    },
    interaction,
};

/// Execute an install (possibly using an existing plan)
#[derive(Debug, Parser)]
pub(crate) struct Install {
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    no_confirm: bool,
    #[clap(flatten)]
    plan_options: PlanOptions,
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub(crate) explain: bool,
    #[clap(
        conflicts_with_all = [ "plan_options" ],
        env = "HARMONIC_PLAN",
    )]
    plan: Option<PathBuf>,
}

#[async_trait::async_trait]
impl CommandExecute for Install {
    #[tracing::instrument(skip_all, fields())]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            no_confirm,
            plan,
            plan_options,
            explain,
        } = self;

        let mut plan = match &plan {
            Some(plan_path) => {
                let install_plan_string = tokio::fs::read_to_string(&plan_path)
                    .await
                    .wrap_err("Reading plan")?;
                serde_json::from_str(&install_plan_string)?
            },
            None => {
                let mut settings = InstallSettings::default()?;

                settings.force(plan_options.force);
                settings.daemon_user_count(plan_options.daemon_user_count);
                settings.channels(
                    plan_options
                        .channel
                        .into_iter()
                        .map(|ChannelValue(name, url)| (name, url)),
                );
                settings.modify_profile(!plan_options.no_modify_profile);

                InstallPlan::new(settings).await?
            },
        };

        if !no_confirm {
            if !interaction::confirm(plan.describe_execute(explain)).await? {
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
