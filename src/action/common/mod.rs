/*! Actions which only call other base plugins. */

mod configure_nix;
mod create_nix_tree;
mod configure_shell_profile;
mod create_users_and_group;
mod place_channel_configuration;
mod place_nix_configuration;
mod provision_nix;

pub use configure_nix::ConfigureNix;
pub use configure_shell_profile::ConfigureShellProfile;
pub use create_nix_tree::{CreateNixTree, CreateNixTreeError};
pub use create_users_and_group::{CreateUsersAndGroup, CreateUsersAndGroupError};
pub use place_channel_configuration::{PlaceChannelConfiguration, PlaceChannelConfigurationError};
pub use place_nix_configuration::{PlaceNixConfiguration, PlaceNixConfigurationError};
pub use provision_nix::{ProvisionNix, ProvisionNixError};
