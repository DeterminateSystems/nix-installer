/*!  [`Action`](crate::action::Action)s for Darwin based systems
*/

pub(crate) mod bootstrap_launchctl_service;
pub(crate) mod configure_remote_building;
pub(crate) mod create_apfs_volume;
pub(crate) mod create_determinate_nix_volume;
pub(crate) mod create_determinate_volume_service;
pub(crate) mod create_fstab_entry;
pub(crate) mod create_nix_hook_service;
pub(crate) mod create_nix_volume;
pub(crate) mod create_synthetic_objects;
pub(crate) mod create_volume_service;
pub(crate) mod enable_ownership;
pub(crate) mod encrypt_apfs_volume;
pub(crate) mod kickstart_launchctl_service;
pub(crate) mod set_tmutil_exclusion;
pub(crate) mod set_tmutil_exclusions;
pub(crate) mod unmount_apfs_volume;

use std::path::Path;
use std::time::Duration;

pub use bootstrap_launchctl_service::BootstrapLaunchctlService;
pub use configure_remote_building::ConfigureRemoteBuilding;
pub use create_apfs_volume::CreateApfsVolume;
pub use create_determinate_nix_volume::CreateDeterminateNixVolume;
pub use create_determinate_volume_service::CreateDeterminateVolumeService;
pub use create_nix_hook_service::CreateNixHookService;
pub use create_nix_volume::{CreateNixVolume, NIX_VOLUME_MOUNTD_DEST};
pub use create_synthetic_objects::CreateSyntheticObjects;
pub use create_volume_service::CreateVolumeService;
pub use enable_ownership::{EnableOwnership, EnableOwnershipError};
pub use encrypt_apfs_volume::EncryptApfsVolume;
pub use kickstart_launchctl_service::KickstartLaunchctlService;
use serde::Deserialize;
pub use set_tmutil_exclusion::SetTmutilExclusion;
pub use set_tmutil_exclusions::SetTmutilExclusions;
use tokio::process::Command;
pub use unmount_apfs_volume::UnmountApfsVolume;
use uuid::Uuid;

use crate::execute_command;

use super::ActionErrorKind;

pub const DARWIN_LAUNCHD_DOMAIN: &str = "system";

async fn get_uuid_for_label(apfs_volume_label: &str) -> Result<Option<Uuid>, ActionErrorKind> {
    let mut command = Command::new("/usr/sbin/diskutil");
    command.process_group(0);
    command.arg("info");
    command.arg("-plist");
    command.arg(apfs_volume_label);
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::piped());

    let command_str = format!("{:?}", command.as_std());

    tracing::trace!(command = command_str, "Executing");
    let output = command
        .output()
        .await
        .map_err(|e| ActionErrorKind::command(&command, e))?;

    let parsed: DiskUtilApfsInfoOutput = plist::from_bytes(&output.stdout)?;

    if let Some(error_message) = parsed.error_message {
        let expected_not_found = format!("Could not find disk: {apfs_volume_label}");
        if error_message.contains(&expected_not_found) {
            Ok(None)
        } else {
            Err(ActionErrorKind::DiskUtilInfoError {
                command: command_str,
                message: error_message,
            })
        }
    } else if let Some(uuid) = parsed.volume_uuid {
        Ok(Some(uuid))
    } else {
        Err(ActionErrorKind::command_output(&command, output))
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
struct DiskUtilApfsInfoOutput {
    #[serde(rename = "ErrorMessage")]
    error_message: Option<String>,
    #[serde(rename = "VolumeUUID")]
    volume_uuid: Option<Uuid>,
}

#[tracing::instrument]
pub(crate) async fn service_is_disabled(
    domain: &str,
    service: &str,
) -> Result<bool, ActionErrorKind> {
    let output = execute_command(
        Command::new("launchctl")
            .process_group(0)
            .arg("print-disabled")
            .arg(domain)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()),
    )
    .await?;
    let utf8_output = String::from_utf8_lossy(&output.stdout);
    let is_disabled = utf8_output.contains(&format!("\"{service}\" => disabled"));
    tracing::trace!(is_disabled, "Service disabled status");
    Ok(is_disabled)
}

/// Waits for the Nix Store mountpoint to exist, up to `retry_tokens * 100ms` amount of time.
#[tracing::instrument]
pub(crate) async fn wait_for_nix_store_dir() -> Result<(), ActionErrorKind> {
    let mut retry_tokens: usize = 150;
    loop {
        let mut command = Command::new("/usr/sbin/diskutil");
        command.process_group(0);
        command.args(["info", "/nix"]);
        command.stderr(std::process::Stdio::null());
        command.stdout(std::process::Stdio::null());
        tracing::trace!(%retry_tokens, command = ?command.as_std(), "Checking for Nix Store mount path existence");
        let output = command
            .output()
            .await
            .map_err(|e| ActionErrorKind::command(&command, e))?;
        if output.status.success() {
            break;
        } else if retry_tokens == 0 {
            return Err(ActionErrorKind::command_output(&command, output))?;
        } else {
            retry_tokens = retry_tokens.saturating_sub(1);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

/// Wait for `launchctl bootstrap {domain} {service}` to succeed up to `retry_tokens * 500ms` amount
/// of time.
#[tracing::instrument]
pub(crate) async fn retry_bootstrap(domain: &str, service: &Path) -> Result<(), ActionErrorKind> {
    let mut retry_tokens: usize = 10;
    loop {
        let mut command = Command::new("launchctl");
        command.process_group(0);
        command.arg("bootstrap");
        command.arg(domain);
        command.arg(service);
        command.stdin(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        command.stdout(std::process::Stdio::null());
        tracing::trace!(%retry_tokens, command = ?command.as_std(), "Waiting for bootstrap to succeed");

        let output = command
            .output()
            .await
            .map_err(|e| ActionErrorKind::command(&command, e))?;

        if output.status.success() {
            break;
        } else if retry_tokens == 0 {
            return Err(ActionErrorKind::command_output(&command, output))?;
        } else {
            retry_tokens = retry_tokens.saturating_sub(1);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Ok(())
}

/// Wait for `launchctl bootout {domain} {service_path}` to succeed up to `retry_tokens * 500ms` amount
/// of time.
#[tracing::instrument]
pub(crate) async fn retry_bootout(
    domain: &str,
    service_path: &Path,
) -> Result<(), ActionErrorKind> {
    let mut retry_tokens: usize = 10;
    loop {
        let mut command = Command::new("launchctl");
        command.process_group(0);
        command.arg("bootout");
        command.arg(domain);
        command.arg(service_path);
        command.stdin(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        command.stdout(std::process::Stdio::null());
        tracing::trace!(%retry_tokens, command = ?command.as_std(), "Waiting for bootout to succeed");

        let output = command
            .output()
            .await
            .map_err(|e| ActionErrorKind::command(&command, e))?;

        if output.status.success() {
            break;
        } else if retry_tokens == 0 {
            return Err(ActionErrorKind::command_output(&command, output))?;
        } else {
            retry_tokens = retry_tokens.saturating_sub(1);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Ok(())
}
