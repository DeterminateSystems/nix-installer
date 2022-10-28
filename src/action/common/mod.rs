/*! Actions which only call other base plugins. */

mod configure_nix;
mod configure_shell_profile;
mod create_directory;
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
mod provision_nix;
mod setup_default_profile;

pub use configure_nix::ConfigureNix;
pub use configure_shell_profile::ConfigureShellProfile;
pub use create_directory::{CreateDirectory, CreateDirectoryError};
pub use create_file::{CreateFile, CreateFileError};
pub use create_group::{CreateGroup, CreateGroupError};
pub use create_nix_tree::{CreateNixTree, CreateNixTreeError};
pub use create_or_append_file::{CreateOrAppendFile, CreateOrAppendFileError};
pub use create_user::{CreateUser, CreateUserError};
pub use create_users_and_group::{CreateUsersAndGroup, CreateUsersAndGroupError};
pub use fetch_nix::{FetchNix, FetchNixError};
pub use move_unpacked_nix::{MoveUnpackedNix, MoveUnpackedNixError};
pub use place_channel_configuration::{PlaceChannelConfiguration, PlaceChannelConfigurationError};
pub use place_nix_configuration::{PlaceNixConfiguration, PlaceNixConfigurationError};
pub use provision_nix::{ProvisionNix, ProvisionNixError};
pub use setup_default_profile::{SetupDefaultProfile, SetupDefaultProfileError};
