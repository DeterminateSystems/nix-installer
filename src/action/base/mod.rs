//! Base [`Action`](crate::action::Action)s that themselves have no other actions as dependencies

pub(crate) mod create_directory;
pub(crate) mod create_file;
pub(crate) mod create_group;
pub(crate) mod create_or_append_file;
pub(crate) mod create_user;
pub(crate) mod fetch_and_unpack_nix;
pub(crate) mod move_unpacked_nix;
pub(crate) mod setup_default_profile;

pub use create_directory::CreateDirectory;
pub use create_file::CreateFile;
pub use create_group::CreateGroup;
pub use create_or_append_file::CreateOrAppendFile;
pub use create_user::CreateUser;
pub use fetch_and_unpack_nix::{FetchAndUnpackNix, FetchUrlError};
pub use move_unpacked_nix::{MoveUnpackedNix, MoveUnpackedNixError};
pub use setup_default_profile::{SetupDefaultProfile, SetupDefaultProfileError};
