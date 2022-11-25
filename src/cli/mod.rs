/*! CLI argument structures and utilities

*/

pub(crate) mod arg;
pub(crate) mod subcommand;

use clap::Parser;
use std::process::ExitCode;
use tokio::sync::broadcast::{Receiver, Sender};

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

pub(crate) async fn signal_channel() -> eyre::Result<(Sender<()>, Receiver<()>)> {
    let (sender, receiver) = tokio::sync::broadcast::channel(100);

    let sender_cloned = sender.clone();
    let _guard = tokio::spawn(async move {
        let mut ctrl_c = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("failed to install signal handler");

        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install signal handler");

        loop {
            tokio::select! {
                    Some(()) = ctrl_c.recv() => {
                        tracing::warn!("Got SIGINT signal");
                        sender_cloned.send(()).ok();
                    },
                    Some(()) = terminate.recv() => {
                        tracing::warn!("Got SIGTERM signal");
                        sender_cloned.send(()).ok();
                    },
            }
        }
    });

    Ok((sender, receiver))
}

pub fn is_root() -> bool {
    nix::unistd::getuid() == nix::unistd::Uid::from_raw(0)
}
