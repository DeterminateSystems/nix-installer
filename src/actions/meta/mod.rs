/*! Actions which only call other base plugins. */

mod configure_nix;
mod configure_shell_profile;
mod create_nix_tree;
mod create_users_and_group;
mod provision_nix;
mod start_nix_daemon;

pub use configure_nix::{ConfigureNix, ConfigureNixReceipt};
pub use configure_shell_profile::{ConfigureShellProfile, ConfigureShellProfileReceipt};
pub use create_nix_tree::{CreateNixTree, CreateNixTreeReceipt};
pub use create_users_and_group::{CreateUsersAndGroup, CreateUsersAndGroupReceipt};
pub use provision_nix::{ProvisionNix, ProvisionNixReceipt};
pub use start_nix_daemon::{StartNixDaemon, StartNixDaemonReceipt};
