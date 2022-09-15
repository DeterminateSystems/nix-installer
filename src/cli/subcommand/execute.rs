use std::process::ExitCode;

use clap::{Parser, ArgAction};
use harmonic::{InstallSettings, InstallPlan};
use tokio::io::{AsyncWriteExt, AsyncReadExt};

use crate::{cli::{arg::ChannelValue, CommandExecute}, interaction};

/// An opinionated, experimental Nix installer
#[derive(Debug, Parser)]
pub(crate) struct Execute {
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    no_confirm: bool
}

#[async_trait::async_trait]
impl CommandExecute for Execute {
    #[tracing::instrument(skip_all, fields(
        
    ))]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self { no_confirm } = self;
        
        let mut stdin = tokio::io::stdin();
        let mut json = String::default();
        stdin.read_to_string(&mut json).await?;
        let plan: InstallPlan = serde_json::from_str(&json)?;
        
        if !no_confirm {
            if !interaction::confirm(
                plan.description()
            )
            .await?
            {
                interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await;
            }
        }



        Ok(ExitCode::SUCCESS)
    }
}