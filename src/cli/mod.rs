/*! CLI argument structures and utilities

*/

pub(crate) mod arg;
mod interaction;
pub(crate) mod subcommand;

use clap::Parser;
use eyre::WrapErr;
use owo_colors::OwoColorize;
use std::{ffi::CString, path::PathBuf, process::ExitCode};
use tokio::sync::broadcast::{Receiver, Sender};
use url::Url;

use self::subcommand::NixInstallerSubcommand;

const FAIL_PKG_SUGGEST: &str = "\
The Determinate Nix Installer failed.

Try our macOS-native package instead, which can handle almost anything:

https://dtr.mn/determinate-nix\
";

#[async_trait::async_trait]
pub trait CommandExecute {
    async fn execute<T>(self, feedback: T) -> eyre::Result<ExitCode>
    where
        T: crate::feedback::Feedback;
}

/**
The Determinate Nix installer

A fast, friendly, and reliable tool to help you use Nix with Flakes everywhere.
*/
#[derive(Debug, Parser)]
#[clap(version)]
pub struct NixInstallerCli {
    /// The proxy to use (if any); valid proxy bases are `https://$URL`, `http://$URL` and `socks5://$URL`
    #[cfg_attr(
        feature = "cli",
        clap(long, env = "NIX_INSTALLER_PROXY", global = true)
    )]
    pub proxy: Option<Url>,

    /// An SSL cert to use (if any); used for fetching Nix and sets `ssl-cert-file` in `/etc/nix/nix.conf`
    #[cfg_attr(
        feature = "cli",
        clap(long, env = "NIX_INSTALLER_SSL_CERT_FILE", global = true)
    )]
    pub ssl_cert_file: Option<PathBuf>,

    #[cfg(feature = "diagnostics")]
    /// Relate the install diagnostic to a specific value
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            default_value = None,
            env = "NIX_INSTALLER_DIAGNOSTIC_ATTRIBUTION",
            global = true
        )
    )]
    pub diagnostic_attribution: Option<String>,

    #[cfg(feature = "diagnostics")]
    /// The URL or file path for an anonymous installation diagnostic to be sent
    ///
    /// To disable diagnostic reporting, unset the default with `--diagnostic-endpoint ""`, or `NIX_INSTALLER_DIAGNOSTIC_ENDPOINT=""`
    #[clap(
        long,
        env = "NIX_INSTALLER_DIAGNOSTIC_ENDPOINT",
        global = true,
        num_args = 0..=1, // Required to allow `--diagnostic-endpoint` or `NIX_INSTALLER_DIAGNOSTIC_ENDPOINT=""`
        default_value = None
    )]
    pub diagnostic_endpoint: Option<String>,

    #[clap(flatten)]
    pub instrumentation: arg::Instrumentation,

    #[clap(subcommand)]
    pub subcommand: NixInstallerSubcommand,
}

#[async_trait::async_trait]
impl CommandExecute for NixInstallerCli {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute<T>(self, mut feedback: T) -> eyre::Result<ExitCode>
    where
        T: crate::feedback::Feedback,
    {
        match self.subcommand {
            NixInstallerSubcommand::Plan(plan) => plan.execute(feedback).await,
            NixInstallerSubcommand::SelfTest(self_test) => self_test.execute(feedback).await,
            NixInstallerSubcommand::Install(install) => {
                let ret = install.execute(feedback.clone()).await;

                if matches!(
                    target_lexicon::OperatingSystem::host(),
                    target_lexicon::OperatingSystem::MacOSX { .. }
                        | target_lexicon::OperatingSystem::Darwin
                ) {
                    #[allow(clippy::collapsible_if)]
                    if ret.is_err() || ret.as_ref().is_ok_and(|code| code == &ExitCode::FAILURE) {
                        let msg = feedback
                            .get_feature_ptr_payload::<String>("dni-det-msg-fail-pkg-ptr")
                            .await
                            .unwrap_or(FAIL_PKG_SUGGEST.into());
                        tracing::warn!("{}", msg);

                        return Ok(ExitCode::FAILURE);
                    }
                }

                ret
            },
            NixInstallerSubcommand::Repair(repair) => repair.execute(feedback).await,
            NixInstallerSubcommand::Uninstall(revert) => revert.execute(feedback).await,
            NixInstallerSubcommand::SplitReceipt(split_receipt) => {
                split_receipt.execute(feedback).await
            },
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
        let set_home_cstring =
            CString::new("--set-home").wrap_err("Making C string of `--set-home`")?;

        let args = std::env::args();
        let mut arg_vec_cstring = vec![];
        arg_vec_cstring.push(sudo_cstring.clone());
        arg_vec_cstring.push(set_home_cstring);

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
                // Our own environments
                key if key.starts_with("DETSYS_") => true,
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
            env_list.push("DETSYS_IDS_IN_CI=1".to_string());
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
