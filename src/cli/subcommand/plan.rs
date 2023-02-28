use std::{path::PathBuf, process::ExitCode};

use crate::{error::HasExpectedErrors, BuiltinPlanner};
use clap::Parser;

use eyre::WrapErr;
use owo_colors::OwoColorize;

use crate::cli::CommandExecute;

/// Plan an install that can be repeated on an identical host later
#[derive(Debug, Parser)]
pub struct Plan {
    #[clap(subcommand)]
    pub planner: Option<BuiltinPlanner>,
    #[clap(env = "NIX_INSTALLER_PLAN", default_value = "/dev/stdout")]
    pub output: PathBuf,
}

#[async_trait::async_trait]
impl CommandExecute for Plan {
    #[tracing::instrument(level = "debug", skip_all, fields())]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self { planner, output } = self;

        let planner = match planner {
            Some(planner) => planner,
            None => BuiltinPlanner::default()
                .await
                .map_err(|e| eyre::eyre!(e))?,
        };

        let res = planner.plan().await;

        let install_plan = match res {
            Ok(plan) => plan,
            Err(e) => {
                if let Some(expected) = e.expected() {
                    eprintln!("{}", expected.red());
                    return Ok(ExitCode::FAILURE);
                }
                return Err(e.into());
            },
        };

        let json = serde_json::to_string_pretty(&install_plan)?;
        tokio::fs::write(output, format!("{json}\n"))
            .await
            .wrap_err("Writing plan")?;

        Ok(ExitCode::SUCCESS)
    }
}
