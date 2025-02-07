use std::process::ExitCode;

use clap::Parser;

use crate::{cli::CommandExecute, NixInstallerError};

/// Run a self test of Nix to ensure that an install is working
#[derive(Debug, Parser)]
pub struct SelfTest {}

#[async_trait::async_trait]
impl CommandExecute for SelfTest {
    #[tracing::instrument(level = "debug", skip_all, fields())]
    async fn execute<T>(self, _feedback: T) -> eyre::Result<ExitCode>
    where
        T: crate::feedback::Feedback,
    {
        crate::self_test::self_test()
            .await
            .map_err(NixInstallerError::SelfTest)?;

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
