//! [`Action`](crate::action::Action)s which only call other base plugins

pub(crate) mod configure_nix;
pub(crate) mod configure_nix_daemon_service;
pub(crate) mod configure_shell_profile;
pub(crate) mod create_nix_tree;
pub(crate) mod create_users_and_groups;
pub(crate) mod place_channel_configuration;
pub(crate) mod place_nix_configuration;
pub(crate) mod provision_nix;

pub use configure_nix::ConfigureNix;
pub use configure_nix_daemon_service::{ConfigureNixDaemonService, ConfigureNixDaemonServiceError};
pub use configure_shell_profile::ConfigureShellProfile;
pub use create_nix_tree::CreateNixTree;
pub use create_users_and_groups::CreateUsersAndGroups;
pub use place_channel_configuration::{PlaceChannelConfiguration, PlaceChannelConfigurationError};
pub use place_nix_configuration::PlaceNixConfiguration;
pub use provision_nix::ProvisionNix;
