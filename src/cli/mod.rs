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

use self::subcommand::NixInstallerSubcommand;

#[async_trait::async_trait]
pub trait CommandExecute {
    async fn execute(self) -> eyre::Result<ExitCode>;
}

/// The Determinate Nix installer
///
/// Plans a Nix install, prompts for confirmation, then executes it
#[derive(Debug, Parser)]
#[clap(version)]
pub struct NixInstallerCli {
    #[clap(flatten)]
    pub instrumentation: arg::Instrumentation,

    #[clap(subcommand)]
    pub subcommand: NixInstallerSubcommand,
}

#[async_trait::async_trait]
impl CommandExecute for NixInstallerCli {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            instrumentation: _,
            subcommand,
        } = self;

        match subcommand {
            NixInstallerSubcommand::Plan(plan) => plan.execute().await,
            NixInstallerSubcommand::SelfTest(self_test) => self_test.execute().await,
            NixInstallerSubcommand::Install(install) => install.execute().await,
            NixInstallerSubcommand::Uninstall(revert) => revert.execute().await,
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
    let euid = nix::unistd::Uid::effective();
    tracing::trace!("Running as EUID {euid}");
    euid.is_root()
}

pub fn ensure_root() -> eyre::Result<()> {
    if !is_root() {
        eprintln!(
            "{}",
            "`nix-installer` needs to run as `root`, attempting to escalate now via `sudo`..."
                .yellow()
                .dimmed()
        );
        let sudo_cstring = CString::new("sudo").wrap_err("Making C string of `sudo`")?;

        let args = std::env::args();
        let mut arg_vec_cstring = vec![];
        arg_vec_cstring.push(sudo_cstring.clone());

        let mut env_list = vec![];
        for (key, value) in std::env::vars() {
            let preserve = match key.as_str() {
                // Rust logging/backtrace bits we use
                "RUST_LOG" | "RUST_BACKTRACE" => true,
                // CI
                "GITHUB_PATH" => true,
                // Used for detecting what command to suggest for sourcing Nix
                "SHELL" => true,
                // Proxy settings (automatically picked up by Reqwest)
                "HTTP_PROXY" | "http_proxy" | "HTTPS_PROXY" | "https_proxy" => true,
                // Our own environments
                key if key.starts_with("NIX_INSTALLER") => true,
                _ => false,
            };
            if preserve {
                env_list.push(format!("{key}={value}"));
            }
        }

        #[cfg(feature = "diagnostics")]
        if is_ci::cached() {
            // Normally `sudo` would erase those envs, so we detect and pass that along specifically to avoid having to pass around
            // a bunch of environment variables
            env_list.push(format!("NIX_INSTALLER_CI=1"));
        }

        if !env_list.is_empty() {
            arg_vec_cstring
                .push(CString::new("env").wrap_err("Building a `env` argument for `sudo`")?);
            for env in env_list {
                arg_vec_cstring.push(
                    CString::new(env.clone())
                        .wrap_err_with(|| format!("Building a `{}` argument for `sudo`", env))?,
                );
            }
        }

        for arg in args {
            arg_vec_cstring.push(CString::new(arg).wrap_err("Making arg into C string")?);
        }

        tracing::trace!("Execvp'ing `{sudo_cstring:?}` with args `{arg_vec_cstring:?}`");
        nix::unistd::execvp(&sudo_cstring, &arg_vec_cstring)
            .wrap_err("Executing `nix-installer` as `root` via `sudo`")?;
    }
    Ok(())
}
