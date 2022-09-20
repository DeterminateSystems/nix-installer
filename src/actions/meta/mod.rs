/*! Actions which only call other base plugins. */

mod create_nix_tree;
mod create_nix_tree_dirs;
mod create_users_and_group;
mod configure_nix;
mod start_nix_daemon;
mod provision_nix;

pub use create_nix_tree::{CreateNixTree, CreateNixTreeReceipt};
pub use create_nix_tree_dirs::{CreateNixTreeDirs, CreateNixTreeDirsReceipt};
pub use create_users_and_group::{CreateUsersAndGroup, CreateUsersAndGroupReceipt};
pub use configure_nix::{ConfigureNix, ConfigureNixReceipt};
pub use start_nix_daemon::{StartNixDaemon, StartNixDaemonReceipt};
pub use provision_nix::{ProvisionNix, ProvisionNixReceipt};