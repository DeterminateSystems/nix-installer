/*! Actions which do not only call other base plugins. */

mod configure_nix_daemon_service;
mod configure_shell_profile;
mod create_directory;
mod create_group;
mod create_user;
mod fetch_nix;
mod move_unpacked_nix;
mod place_channel_configuration;
mod place_nix_configuration;
mod setup_default_profile;
mod start_systemd_service;

pub use configure_nix_daemon_service::{
    ConfigureNixDaemonService, ConfigureNixDaemonServiceReceipt,
};
pub use configure_shell_profile::{ConfigureShellProfile, ConfigureShellProfileReceipt};
pub use create_directory::{CreateDirectory, CreateDirectoryReceipt};
pub use create_group::{CreateGroup, CreateGroupReceipt};
pub use create_user::{CreateUser, CreateUserReceipt};
pub use fetch_nix::{FetchNix, FetchNixReceipt};
pub use move_unpacked_nix::{MoveUnpackedNix, MoveUnpackedNixReceipt};
pub use place_channel_configuration::{
    PlaceChannelConfiguration, PlaceChannelConfigurationReceipt,
};
pub use place_nix_configuration::{PlaceNixConfiguration, PlaceNixConfigurationReceipt};
pub use setup_default_profile::{SetupDefaultProfile, SetupDefaultProfileReceipt};
pub use start_systemd_service::{StartSystemdService, StartSystemdServiceReceipt};