pub(crate) mod arg;
pub(crate) mod subcommand;

use crate::interaction;
use clap::Parser;
use harmonic::Harmonic;
use reqwest::Url;
use std::process::ExitCode;

#[async_trait::async_trait]
pub(crate) trait CommandExecute {
    async fn execute(self) -> eyre::Result<ExitCode>;
}

#[derive(Debug, Parser)]
#[clap(version)]
pub(crate) struct HarmonicCli {
    #[clap(flatten)]
    pub(crate) instrumentation: arg::Instrumentation,
    #[clap(long, default_value = "https://nixos.org/channels/nixpkgs-unstable")]
    pub(crate) channels: Vec<Url>,
    #[clap(long)]
    pub(crate) no_modify_profile: bool,
    #[clap(long, default_value = "32")]
    pub(crate) daemon_user_count: usize,
    #[clap(subcommand)]
    subcommand: Option<subcommand::Subcommand>,
}

#[async_trait::async_trait]
impl CommandExecute for HarmonicCli {
    #[tracing::instrument(skip_all, fields(
        channels = %self.channels.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "),
        daemon_user_count = %self.daemon_user_count,
        no_modify_profile = %self.no_modify_profile,
    ))]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            instrumentation: _,
            daemon_user_count,
            channels,
            no_modify_profile,
            subcommand,
        } = self;

        match subcommand {
            Some(subcommand::Subcommand::NixOs(nixos)) => return nixos.execute().await,
            None => (),
        }

        let mut harmonic = Harmonic::default();

        harmonic.daemon_user_count(daemon_user_count);
        harmonic.channels(channels);
        harmonic.modify_profile(!no_modify_profile);

        if !interaction::confirm("Are you ready to continue?").await? {
            interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await;
        }

        harmonic.create_group().await?;
        harmonic.create_users().await?;
        harmonic.create_directories().await?;
        harmonic.place_channel_configuration().await?;
        harmonic.fetch_nix().await?;
        harmonic.configure_shell_profile().await?;
        harmonic.setup_default_profile().await?;
        harmonic.place_nix_configuration().await?;

        Ok(ExitCode::SUCCESS)
    }
}
