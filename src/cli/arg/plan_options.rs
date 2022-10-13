use clap::{ArgAction, Parser};

/// Plan an install that can be repeated on an identical host later
#[derive(Debug, Parser)]
pub(crate) struct PlanOptions {
    /// Channel(s) to add by default, pass multiple times for multiple channels
    #[clap(
        long,
        value_parser,
        action = clap::ArgAction::Append,
        env = "HARMONIC_CHANNEL",
        default_value = "nixpkgs=https://nixos.org/channels/nixpkgs-unstable",
        group = "plan_options"
    )]
    pub(crate) channel: Vec<crate::cli::arg::ChannelValue>,
    /// Don't modify the user profile to automatically load nix
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true,
        group = "plan_options"
    )]
    pub(crate) no_modify_profile: bool,
    /// Number of build users to create
    #[clap(
        long,
        default_value = "32",
        env = "HARMONIC_NIX_DAEMON_USER_COUNT",
        group = "plan_options"
    )]
    pub(crate) daemon_user_count: usize,
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true,
        group = "plan_options"
    )]
    pub(crate) force: bool,
}
