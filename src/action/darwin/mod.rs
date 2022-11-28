/*!  [`Action`](crate::action::Action)s for Darwin based systems
*/

mod bootstrap_apfs_volume;
mod create_apfs_volume;
mod create_nix_volume;
mod create_synthetic_objects;
mod enable_ownership;
mod encrypt_apfs_volume;
mod kickstart_launchctl_service;
mod unmount_apfs_volume;

pub use bootstrap_apfs_volume::{BootstrapApfsVolume, BootstrapVolumeError};
pub use create_apfs_volume::{CreateApfsVolume, CreateVolumeError};
pub use create_nix_volume::{CreateApfsVolumeError, CreateNixVolume, NIX_VOLUME_MOUNTD_DEST};
pub use create_synthetic_objects::{CreateSyntheticObjects, CreateSyntheticObjectsError};
pub use enable_ownership::{EnableOwnership, EnableOwnershipError};
pub use encrypt_apfs_volume::{EncryptApfsVolume, EncryptVolumeError};
pub use kickstart_launchctl_service::{KickstartLaunchctlService, KickstartLaunchctlServiceError};
pub use unmount_apfs_volume::{UnmountApfsVolume, UnmountVolumeError};
