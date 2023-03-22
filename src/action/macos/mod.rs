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
pub(crate) mod setup_volume_daemon;
pub(crate) mod unmount_apfs_volume;

pub use bootstrap_launchctl_service::BootstrapLaunchctlService;
pub use create_apfs_volume::CreateApfsVolume;
pub use create_nix_volume::{CreateNixVolume, NIX_VOLUME_MOUNTD_DEST};
pub use create_synthetic_objects::{CreateSyntheticObjects, CreateSyntheticObjectsError};
pub use enable_ownership::{EnableOwnership, EnableOwnershipError};
pub use encrypt_apfs_volume::EncryptApfsVolume;
pub use kickstart_launchctl_service::KickstartLaunchctlService;
use serde::Deserialize;
pub use setup_volume_daemon::SetupVolumeDaemon;
use tokio::process::Command;
pub use unmount_apfs_volume::UnmountApfsVolume;
use uuid::Uuid;

use crate::execute_command;

use super::ActionError;

/// This function must be able to operate at both plan and execute time!
async fn get_uuid_for_label(apfs_volume_label: &str) -> Result<Uuid, ActionError> {
    let output = execute_command(
        Command::new("/usr/sbin/diskutil")
            .process_group(0)
            .arg("info")
            .arg("-plist")
            .arg(apfs_volume_label)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped()),
    )
    .await?;

    let parsed: DiskUtilApfsInfoOutput = plist::from_bytes(&output.stdout)?;

    Ok(parsed.volume_uuid)
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
struct DiskUtilApfsInfoOutput {
    #[serde(rename = "VolumeUUID")]
    volume_uuid: Uuid,
}
