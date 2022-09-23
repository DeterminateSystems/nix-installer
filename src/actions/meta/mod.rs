/*! Actions which only call other base plugins. */

mod configure_nix;
mod configure_shell_profile;
mod create_nix_tree;
mod create_users_and_group;
mod provision_nix;
mod start_nix_daemon;

pub use configure_nix::{ConfigureNix, ConfigureNixError};
pub use configure_shell_profile::{ConfigureShellProfile, ConfigureShellProfileError};
pub use create_nix_tree::{CreateNixTree, CreateNixTreeError};
pub use create_users_and_group::{CreateUsersAndGroup, CreateUsersAndGroupError};
pub use provision_nix::{ProvisionNix, ProvisionNixError};
pub use start_nix_daemon::{StartNixDaemon, StartNixDaemonError};
