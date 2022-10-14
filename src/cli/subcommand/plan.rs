use std::{path::PathBuf, process::ExitCode};

use clap::Parser;
use harmonic::{InstallPlan, InstallSettings, Planner};

use eyre::WrapErr;

use crate::cli::{
    arg::{ChannelValue, PlanOptions},
    CommandExecute,
};

/// Plan an install that can be repeated on an identical host later
#[derive(Debug, Parser)]
pub(crate) struct Plan {
    #[clap(flatten)]
    plan_options: PlanOptions,
    #[clap(default_value = "/dev/stdout")]
    pub(crate) plan: PathBuf,
}

#[async_trait::async_trait]
impl CommandExecute for Plan {
    #[tracing::instrument(skip_all, fields(
        channels = %self.plan_options.channel.iter().map(|ChannelValue(name, url)| format!("{name} {url}")).collect::<Vec<_>>().join(", "),
        daemon_user_count = %self.plan_options.daemon_user_count,
        no_modify_profile = %self.plan_options.no_modify_profile,
    ))]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            plan_options:
                PlanOptions {
                    channel,
                    no_modify_profile,
                    daemon_user_count,
                    force,
                    planner,
                },
            plan,
        } = self;

        let mut settings = InstallSettings::default()?;

        settings.force(force);
        settings.daemon_user_count(daemon_user_count);
        settings.channels(
            channel
                .into_iter()
                .map(|ChannelValue(name, url)| (name, url)),
        );
        settings.modify_profile(!no_modify_profile);

        let planner = match planner {
            Some(planner) => planner,
            None => Planner::default()?,
        };

        let install_plan = InstallPlan::new(planner, settings).await?;

        let json = serde_json::to_string_pretty(&install_plan)?;
        tokio::fs::write(plan, json)
            .await
            .wrap_err("Writing plan")?;

        Ok(ExitCode::SUCCESS)
    }
}
