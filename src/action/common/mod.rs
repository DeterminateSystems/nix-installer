//! [`Action`](crate::action::Action)s which only call other base plugins

mod configure_nix;
mod configure_shell_profile;
mod create_nix_tree;
mod create_users_and_groups;
mod place_channel_configuration;
mod place_nix_configuration;
mod provision_nix;

pub use configure_nix::ConfigureNix;
pub use configure_shell_profile::ConfigureShellProfile;
pub use create_nix_tree::CreateNixTree;
pub use create_users_and_groups::CreateUsersAndGroups;
pub use place_channel_configuration::{PlaceChannelConfiguration, PlaceChannelConfigurationError};
pub use place_nix_configuration::PlaceNixConfiguration;
pub use provision_nix::ProvisionNix;
