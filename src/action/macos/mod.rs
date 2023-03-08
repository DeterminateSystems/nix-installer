/*!  [`Action`](crate::action::Action)s for Darwin based systems
*/

pub(crate) mod bootstrap_launchctl_service;
pub(crate) mod create_apfs_volume;
pub(crate) mod create_fstab_entry;
pub(crate) mod create_nix_volume;
pub(crate) mod create_synthetic_objects;
pub(crate) mod enable_ownership;
pub(crate) mod encrypt_apfs_volume;
pub(crate) mod kickstart_launchctl_service;
pub(crate) mod unmount_apfs_volume;

pub use bootstrap_launchctl_service::BootstrapLaunchctlService;
pub use create_apfs_volume::CreateApfsVolume;
pub use create_nix_volume::{CreateNixVolume, NIX_VOLUME_MOUNTD_DEST};
pub use create_synthetic_objects::{CreateSyntheticObjects, CreateSyntheticObjectsError};
pub use enable_ownership::{EnableOwnership, EnableOwnershipError};
pub use encrypt_apfs_volume::EncryptApfsVolume;
pub use kickstart_launchctl_service::KickstartLaunchctlService;
pub use unmount_apfs_volume::UnmountApfsVolume;
