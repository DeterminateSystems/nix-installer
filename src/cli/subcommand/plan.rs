use std::{path::PathBuf, process::ExitCode};

use crate::BuiltinPlanner;
use clap::Parser;

use eyre::WrapErr;

use crate::cli::CommandExecute;

/// Plan an install that can be repeated on an identical host later
#[derive(Debug, Parser)]
pub struct Plan {
    #[clap(subcommand)]
    pub planner: Option<BuiltinPlanner>,
    #[clap(env = "HARMONIC_PLAN", default_value = "/dev/stdout")]
    pub output: PathBuf,
}

#[async_trait::async_trait]
impl CommandExecute for Plan {
    #[tracing::instrument(skip_all, fields())]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self { planner, output } = self;

        let planner = match planner {
            Some(planner) => planner,
            None => BuiltinPlanner::default()
                .await
                .map_err(|e| eyre::eyre!(e))?,
        };

        let install_plan = planner.plan().await.map_err(|e| eyre::eyre!(e))?;

        let json = serde_json::to_string_pretty(&install_plan)?;
        tokio::fs::write(output, json)
            .await
            .wrap_err("Writing plan")?;

        Ok(ExitCode::SUCCESS)
    }
}
