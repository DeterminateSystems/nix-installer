//! Top level actions invoked by an install/ uninstall plan

mod configure_nix;
mod create_directory;
mod provision_nix;

pub mod darwin;
pub mod linux;

pub use configure_nix::ConfigureNix;
pub use create_directory::{CreateDirectory, CreateDirectoryError};
pub use provision_nix::{ProvisionNix, ProvisionNixError};
