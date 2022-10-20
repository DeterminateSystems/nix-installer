/*! Actions which only call other base plugins. */

mod configure_nix;
mod configure_shell_profile;
mod create_nix_tree;
mod create_systemd_sysext;
mod create_users_and_group;
pub mod darwin;
mod place_channel_configuration;
mod place_nix_configuration;
mod provision_nix;

pub use configure_nix::{ConfigureNix, ConfigureNixError};
pub use configure_shell_profile::{ConfigureShellProfile, ConfigureShellProfileError};
pub use create_nix_tree::{CreateNixTree, CreateNixTreeError};
pub use create_systemd_sysext::{CreateSystemdSysext, CreateSystemdSysextError};
pub use create_users_and_group::{CreateUsersAndGroup, CreateUsersAndGroupError};
pub use place_channel_configuration::{PlaceChannelConfiguration, PlaceChannelConfigurationError};
pub use place_nix_configuration::{PlaceNixConfiguration, PlaceNixConfigurationError};
pub use provision_nix::{ProvisionNix, ProvisionNixError};
