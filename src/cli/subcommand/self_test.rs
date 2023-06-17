use std::process::ExitCode;

use clap::Parser;

use crate::cli::CommandExecute;

/// Run a self test of Nix to ensure that the install worked.
#[derive(Debug, Parser)]
pub struct SelfTest {}

#[async_trait::async_trait]
impl CommandExecute for SelfTest {
    #[tracing::instrument(level = "debug", skip_all, fields())]
    async fn execute(self) -> eyre::Result<ExitCode> {
        crate::self_test::self_test().await?;

        tracing::info!(
            shells = ?crate::self_test::Shell::discover()
                .iter()
                .map(|v| v.executable())
                .collect::<Vec<_>>(),
            "Successfully tested Nix install in all discovered shells."
        );
        Ok(ExitCode::SUCCESS)
    }
}
