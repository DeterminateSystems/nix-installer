pub(crate) mod arg;
pub(crate) mod subcommand;

use clap::Parser;
use std::process::ExitCode;

use self::subcommand::HarmonicSubcommand;

#[async_trait::async_trait]
pub trait CommandExecute {
    async fn execute(self) -> eyre::Result<ExitCode>;
}

/// An opinionated, experimental Nix installer
///
/// Plans a Nix install, prompts for confirmation, then executes it
#[derive(Debug, Parser)]
#[clap(version)]
pub struct HarmonicCli {
    #[clap(flatten)]
    pub instrumentation: arg::Instrumentation,

    #[clap(subcommand)]
    pub subcommand: HarmonicSubcommand,
}

#[async_trait::async_trait]
impl CommandExecute for HarmonicCli {
    #[tracing::instrument(skip_all)]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            instrumentation: _,
            subcommand,
        } = self;

        match subcommand {
            HarmonicSubcommand::Plan(plan) => plan.execute().await,
            HarmonicSubcommand::Install(install) => install.execute().await,
            HarmonicSubcommand::Uninstall(revert) => revert.execute().await,
        }
    }
}

pub fn is_root() -> bool {
    nix::unistd::getuid() == nix::unistd::Uid::from_raw(0)
}
