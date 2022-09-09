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

/// An opinionated, experimental Nix installer
#[derive(Debug, Parser)]
#[clap(version)]
pub(crate) struct HarmonicCli {
    // Don't actually install, just log expected actions
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub(crate) dry_run: bool,
    #[clap(flatten)]
    pub(crate) instrumentation: arg::Instrumentation,
    /// Channel(s) to add by default, pass multiple times for multiple channels
    #[clap(
        long,
        value_parser,
        action = clap::ArgAction::Append,
        env = "HARMONIC_CHANNEL",
        default_value = "nixpkgs=https://nixos.org/channels/nixpkgs-unstable"
    )]
    pub(crate) channel: Vec<arg::ChannelValue>,
    /// Don't modify the user profile to automatically load nix
    #[clap(long)]
    pub(crate) no_modify_profile: bool,
    /// Number of build users to create
    #[clap(long, default_value = "32", env = "HARMONIC_NIX_DAEMON_USER_COUNT")]
    pub(crate) daemon_user_count: usize,
    #[cfg(target_os = "linux")]
    #[clap(subcommand)]
    subcommand: Option<subcommand::Subcommand>,
}

#[async_trait::async_trait]
impl CommandExecute for HarmonicCli {
    #[tracing::instrument(skip_all, fields(
        channels = %self.channel.iter().map(|ChannelValue(name, url)| format!("{name} {url}")).collect::<Vec<_>>().join(", "),
        daemon_user_count = %self.daemon_user_count,
        no_modify_profile = %self.no_modify_profile,
        dry_run = %self.dry_run,
    ))]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            dry_run,
            instrumentation: _,
            daemon_user_count,
            channel,
            no_modify_profile,
            subcommand,
        } = self;

        #[cfg(target_os = "linux")]
        match subcommand {
            Some(subcommand::Subcommand::NixOs(nixos)) => return nixos.execute().await,
            None => (),
        }

        let mut harmonic = Harmonic::default();

        harmonic.dry_run(dry_run);
        harmonic.daemon_user_count(daemon_user_count);
        harmonic.channels(
            channel
                .into_iter()
                .map(|ChannelValue(name, url)| (name, url)),
        );
        harmonic.modify_profile(!no_modify_profile);

        // TODO(@Hoverbear): Make this smarter
        if !interaction::confirm(
            "\
            Ready to install nix?\n\
            \n\
            This installer will:\n\
            \n\
            * Create a `nixbld` group\n\
            * Create several `nixbld*` users\n\
            * Create several Nix related directories\n\
            * Place channel configurations\n\
            * Fetch a copy of Nix and unpack it\n\
            * Configure the shell profiles of various shells\n\
            * Place a Nix configuration\n\
            * Configure the Nix daemon to work with your init\
        ",
        )
        .await?
        {
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
