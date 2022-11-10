//! Base actions that themselves have no other actions as dependencies

mod configure_nix_daemon_service;
mod create_directory;
mod create_file;
mod create_group;
mod create_or_append_file;
mod create_user;
mod fetch_nix;
mod move_unpacked_nix;
mod setup_default_profile;

pub use configure_nix_daemon_service::{ConfigureNixDaemonService, ConfigureNixDaemonServiceError};
pub use create_directory::{CreateDirectory, CreateDirectoryError};
pub use create_file::{CreateFile, CreateFileError};
pub use create_group::{CreateGroup, CreateGroupError};
pub use create_or_append_file::{CreateOrAppendFile, CreateOrAppendFileError};
pub use create_user::{CreateUser, CreateUserError};
pub use fetch_nix::{FetchNix, FetchNixError};
pub use move_unpacked_nix::{MoveUnpackedNix, MoveUnpackedNixError};
pub use setup_default_profile::{SetupDefaultProfile, SetupDefaultProfileError};
