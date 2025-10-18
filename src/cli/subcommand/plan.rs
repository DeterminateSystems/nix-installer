use std::{path::PathBuf, process::ExitCode};

use crate::{cli::ensure_root, error::HasExpectedErrors, BuiltinPlanner};
use clap::Parser;

use eyre::WrapErr;
use owo_colors::OwoColorize;

use crate::cli::CommandExecute;

/**
Emit a JSON install plan that can be manually edited before execution

Primarily intended for development, debugging, and handling install cases.
*/
#[derive(Debug, Parser)]
pub struct Plan {
    #[clap(subcommand)]
    pub planner: Option<BuiltinPlanner>,
    /// Where to write the generated plan (in JSON format)
    #[clap(
        long = "out-file",
        env = "NIX_INSTALLER_PLAN_OUT_FILE",
        default_value = "/dev/stdout"
    )]
    pub output: PathBuf,
}

#[async_trait::async_trait]
impl CommandExecute for Plan {
    #[tracing::instrument(level = "debug", skip_all, fields())]
    async fn execute<T>(self, mut feedback: T) -> eyre::Result<ExitCode>
    where
        T: crate::feedback::Feedback,
    {
        let Self { planner, output } = self;

        ensure_root()?;

        let planner = match planner {
            Some(planner) => planner,
            None => BuiltinPlanner::default().await?,
        };

        feedback.set_planner(&planner).await?;

        let res = planner.plan().await;

        let install_plan = match res {
            Ok(plan) => plan,
            Err(err) => {
                feedback.planning_failed(&err).await;
                if let Some(expected) = err.expected() {
                    eprintln!("{}", expected.red());
                    return Ok(ExitCode::FAILURE);
                }
                return Err(err)?;
            },
        };

        feedback.planning_succeeded().await;

        let json = serde_json::to_string_pretty(&install_plan)?;
        tokio::fs::write(output, format!("{json}\n"))
            .await
            .wrap_err("Writing plan")?;

        Ok(ExitCode::SUCCESS)
    }
}
