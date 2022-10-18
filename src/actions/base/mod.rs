/*! Actions which do not only call other base plugins. */

mod configure_nix_daemon_service;
mod create_directory;
mod create_file;
mod create_group;
mod create_or_append_file;
mod create_user;
pub mod darwin;
mod fetch_nix;
mod move_unpacked_nix;
mod setup_default_profile;
mod start_systemd_unit;
mod systemd_sysext_merge;

pub use configure_nix_daemon_service::{ConfigureNixDaemonService, ConfigureNixDaemonServiceError};
pub use create_directory::{CreateDirectory, CreateDirectoryError};
pub use create_file::{CreateFile, CreateFileError};
pub use create_group::{CreateGroup, CreateGroupError};
pub use create_or_append_file::{CreateOrAppendFile, CreateOrAppendFileError};
pub use create_user::{CreateUser, CreateUserError};
pub use fetch_nix::{FetchNix, FetchNixError};
pub use move_unpacked_nix::{MoveUnpackedNix, MoveUnpackedNixError};
pub use setup_default_profile::{SetupDefaultProfile, SetupDefaultProfileError};
pub use start_systemd_unit::{StartSystemdUnit, StartSystemdUnitError};
pub use systemd_sysext_merge::{SystemdSysextMerge, SystemdSysextMergeError};
