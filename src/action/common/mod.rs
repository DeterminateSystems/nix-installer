/*! Actions which only call other base plugins. */

mod configure_nix_daemon_service;
mod configure_shell_profile;
mod create_file;
mod create_group;
mod create_nix_tree;
mod create_or_append_file;
mod create_user;
mod create_users_and_group;
mod fetch_nix;
mod move_unpacked_nix;
mod place_channel_configuration;
mod place_nix_configuration;
mod setup_default_profile;

pub(in crate::action) use configure_nix_daemon_service::{
    ConfigureNixDaemonService, ConfigureNixDaemonServiceError,
};
pub(in crate::action) use configure_shell_profile::ConfigureShellProfile;
pub(in crate::action) use create_file::{CreateFile, CreateFileError};
pub(in crate::action) use create_group::{CreateGroup, CreateGroupError};
pub(in crate::action) use create_nix_tree::{CreateNixTree, CreateNixTreeError};
pub(in crate::action) use create_or_append_file::{CreateOrAppendFile, CreateOrAppendFileError};
pub(in crate::action) use create_user::{CreateUser, CreateUserError};
pub(in crate::action) use create_users_and_group::{CreateUsersAndGroup, CreateUsersAndGroupError};
pub(in crate::action) use fetch_nix::{FetchNix, FetchNixError};
pub(in crate::action) use move_unpacked_nix::{MoveUnpackedNix, MoveUnpackedNixError};
pub(in crate::action) use place_channel_configuration::{
    PlaceChannelConfiguration, PlaceChannelConfigurationError,
};
pub(in crate::action) use place_nix_configuration::{
    PlaceNixConfiguration, PlaceNixConfigurationError,
};
pub(in crate::action) use setup_default_profile::{SetupDefaultProfile, SetupDefaultProfileError};
