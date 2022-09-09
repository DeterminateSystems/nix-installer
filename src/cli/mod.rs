pub(crate) mod arg;
pub(crate) mod subcommand;

use crate::{cli::arg::ChannelValue, interaction};
use clap::{ArgAction, Parser};
use harmonic::Harmonic;
use std::process::ExitCode;

#[async_trait::async_trait]
pub(crate) trait CommandExecute {
    async fn execute(self) -> eyre::Result<ExitCode>;
}

#[derive(Debug, Parser)]
#[clap(version)]
pub(crate) struct HarmonicCli {
    #[clap(long, action(ArgAction::SetTrue), default_value = "false")]
    pub(crate) dry_run: bool,
    #[clap(flatten)]
    pub(crate) instrumentation: arg::Instrumentation,
    #[clap(
        long,
        value_parser,
        default_value = "nixpkgs=https://nixos.org/channels/nixpkgs-unstable"
    )]
    pub(crate) channels: Vec<arg::ChannelValue>,
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
        channels = %self.channels.iter().map(|ChannelValue(name, url)| format!("{name} {url}")).collect::<Vec<_>>().join(", "),
        daemon_user_count = %self.daemon_user_count,
        no_modify_profile = %self.no_modify_profile,
        dry_run = %self.dry_run,
    ))]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            dry_run,
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

        harmonic.dry_run(dry_run);
        harmonic.daemon_user_count(daemon_user_count);
        harmonic.channels(
            channels
                .into_iter()
                .map(|ChannelValue(name, url)| (name, url)),
        );
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

        harmonic.configure_nix_daemon_service().await?;

        Ok(ExitCode::SUCCESS)
    }
}
