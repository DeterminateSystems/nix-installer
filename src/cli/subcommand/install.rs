use std::{path::PathBuf, process::ExitCode};

use clap::{ArgAction, Parser};
use eyre::{eyre, WrapErr};
use harmonic::{InstallPlan, InstallSettings};

use crate::{
    cli::{arg::ChannelValue, CommandExecute},
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
    /// Channel(s) to add by default, pass multiple times for multiple channels
    #[clap(
        long,
        value_parser,
        action = clap::ArgAction::Append,
        env = "HARMONIC_CHANNEL",
        default_value = "nixpkgs=https://nixos.org/channels/nixpkgs-unstable"
    )]
    pub(crate) channel: Vec<ChannelValue>,
    /// Don't modify the user profile to automatically load nix
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub(crate) no_modify_profile: bool,
    /// Number of build users to create
    #[clap(long, default_value = "32", env = "HARMONIC_NIX_DAEMON_USER_COUNT")]
    pub(crate) daemon_user_count: usize,
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub(crate) explain: bool,
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub(crate) force: bool,
    #[clap(
        conflicts_with_all = [ "no_modify_profile", "daemon_user_count", "channel" ],
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
            explain,
            channel,
            no_modify_profile,
            daemon_user_count,
            force,
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

                settings.force(force);
                settings.daemon_user_count(daemon_user_count);
                settings.channels(
                    channel
                        .into_iter()
                        .map(|ChannelValue(name, url)| (name, url)),
                );
                settings.modify_profile(!no_modify_profile);

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
