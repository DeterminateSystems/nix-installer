use std::process::ExitCode;

use clap::{ArgAction, Parser};
use harmonic::{InstallPlan, InstallSettings};
use tokio::io::AsyncWriteExt;

use crate::cli::{arg::ChannelValue, CommandExecute};

/// An opinionated, experimental Nix installer
#[derive(Debug, Parser)]
pub(crate) struct Plan {
    /// Channel(s) to add by default, pass multiple times for multiple channels
    #[clap(
        long,
        value_parser,
        action = clap::ArgAction::Append,
        env = "HARMONIC_CHANNEL",
        default_value = "nixpkgs=https://nixos.org/channels/nixpkgs-unstable"
    )]
    pub(crate) channel: Vec<crate::cli::arg::ChannelValue>,
    /// Don't modify the user profile to automatically load nix
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub(crate) no_modify_profile: bool,
    /// Number of build users to create
    #[clap(long, default_value = "32", env = "HARMONIC_NIX_DAEMON_USER_COUNT")]
    pub(crate) daemon_user_count: usize,
}

#[async_trait::async_trait]
impl CommandExecute for Plan {
    #[tracing::instrument(skip_all, fields(
        channels = %self.channel.iter().map(|ChannelValue(name, url)| format!("{name} {url}")).collect::<Vec<_>>().join(", "),
        daemon_user_count = %self.daemon_user_count,
        no_modify_profile = %self.no_modify_profile,
    ))]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            channel,
            no_modify_profile,
            daemon_user_count,
        } = self;

        let mut settings = InstallSettings::default();

        settings.daemon_user_count(daemon_user_count);
        settings.channels(
            channel
                .into_iter()
                .map(|ChannelValue(name, url)| (name, url)),
        );
        settings.modify_profile(!no_modify_profile);

        let plan = InstallPlan::new(settings).await?;

        let json = serde_json::to_string_pretty(&plan)?;
        let mut stdout = tokio::io::stdout();
        stdout.write_all(json.as_bytes()).await?;
        Ok(ExitCode::SUCCESS)
    }
}
