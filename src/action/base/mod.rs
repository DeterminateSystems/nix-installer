//! Base [`Action`](crate::action::Action)s that themselves have no other actions as dependencies

pub(crate) mod add_user_to_group;
pub(crate) mod create_directory;
pub(crate) mod create_file;
pub(crate) mod create_group;
pub(crate) mod create_or_insert_into_file;
pub(crate) mod create_or_merge_nix_config;
pub(crate) mod create_user;
pub(crate) mod delete_user;
pub(crate) mod fetch_and_unpack_nix;
pub(crate) mod move_unpacked_nix;
pub(crate) mod remove_directory;
pub(crate) mod setup_default_profile;

pub use add_user_to_group::AddUserToGroup;
pub use create_directory::CreateDirectory;
pub use create_file::CreateFile;
pub use create_group::CreateGroup;
pub use create_or_insert_into_file::CreateOrInsertIntoFile;
pub use create_or_merge_nix_config::CreateOrMergeNixConfig;
pub use create_user::CreateUser;
pub use delete_user::DeleteUser;
pub use fetch_and_unpack_nix::{FetchAndUnpackNix, FetchUrlError};
pub use move_unpacked_nix::{MoveUnpackedNix, MoveUnpackedNixError};
pub use remove_directory::RemoveDirectory;
pub use setup_default_profile::{SetupDefaultProfile, SetupDefaultProfileError};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, PartialEq, Eq)]
struct PublishedPathIdentity {
    inode: u64,
    created: Option<std::time::SystemTime>,
}

impl From<&std::fs::Metadata> for PublishedPathIdentity {
    fn from(metadata: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;

        // Device numbers can change across boots.
        Self {
            inode: metadata.ino(),
            created: metadata.created().ok(),
        }
    }
}

#[cfg(target_os = "macos")]
fn validate_no_extended_acl(
    file: &std::fs::File,
    path: &std::path::Path,
) -> Result<(), super::ActionErrorKind> {
    use nix::libc::{c_int, c_void, ENOENT};
    use std::{io, os::fd::AsRawFd};

    extern "C" {
        fn acl_get_fd(fd: c_int) -> *mut c_void;
        fn acl_free(acl: *mut c_void) -> c_int;
    }

    // SAFETY: the borrowed file keeps the descriptor open throughout the call.
    let acl = unsafe { acl_get_fd(file.as_raw_fd()) };
    if acl.is_null() {
        let error = io::Error::last_os_error();
        // Darwin reports a missing ACL as ENOENT even for a valid open descriptor.
        return if error.raw_os_error() == Some(ENOENT) {
            Ok(())
        } else {
            Err(super::ActionErrorKind::GettingMetadata(path.into(), error))
        };
    }
    // SAFETY: acl_get_fd returned an allocated ACL owned by this function.
    unsafe { acl_free(acl) };
    // Keep the validation independent of ACL ordering, inheritance, and membership.
    Err(super::ActionErrorKind::UnexpectedAcl(path.into()))
}

#[cfg(not(target_os = "macos"))]
fn validate_no_extended_acl(
    _: &std::fs::File,
    _: &std::path::Path,
) -> Result<(), super::ActionErrorKind> {
    Ok(())
}
