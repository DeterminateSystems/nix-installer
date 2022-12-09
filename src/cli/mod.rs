/*! CLI argument structures and utilities

*/

pub(crate) mod arg;
mod interaction;
pub(crate) mod subcommand;

use clap::Parser;
use eyre::WrapErr;
use owo_colors::OwoColorize;
use std::{ffi::CString, process::ExitCode};
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
    nix::unistd::geteuid() == nix::unistd::Uid::from_raw(0)
}

pub fn ensure_root() -> eyre::Result<()> {
    if !is_root() {
        eprintln!(
            "{}",
            "Harmonic needs to run as `root` (usually via `sudo`), attempting to escalate you now with `sudo`..."
                .yellow()
                .dimmed()
        );
        let current_exe =
            std::env::current_exe().wrap_err("Could not get current executable path")?;
        let args = std::env::args();
        let mut arg_vec_cstring = vec![];
        arg_vec_cstring.push(CString::new("sudo").wrap_err("Making `sudo` into C string")?);
        arg_vec_cstring.push(
            CString::new(current_exe.to_string_lossy().into_owned())
                .wrap_err("Making current executable into C string")?,
        );
        for arg in args.skip(1) {
            arg_vec_cstring.push(CString::new(arg).wrap_err("Making arg into C string")?);
        }
        let env_cstring = CString::new("/usr/bin/env")
            .wrap_err("Making C string of executable `/usr/bin/env`")?;

        tracing::trace!("Execv'ing `{env_cstring:?} {arg_vec_cstring:?}`");
        nix::unistd::execv(&env_cstring, &arg_vec_cstring)
            .wrap_err("Executing Harmonic as `root` via `sudo`")?;
    }
    Ok(())
}
