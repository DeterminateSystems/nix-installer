use std::time::Duration;

use crate::{
    action::{
        macos::NIX_VOLUME_MOUNTD_DEST, Action, ActionDescription, ActionError, ActionErrorKind,
        ActionState, ActionTag, StatefulAction,
    },
    distribution::Distribution,
    execute_command,
    os::darwin::DiskUtilApfsListOutput,
};
use rand::Rng;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{io::AsyncWriteExt as _, process::Command};
use tracing::{span, Span};

use super::{CreateApfsVolume, KEYCHAIN_NIX_STORE_SERVICE};

/**
Encrypt an APFS volume
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "encrypt_apfs_volume")]
pub struct EncryptApfsVolume {
    distribution: Distribution,
    disk: PathBuf,
    name: String,
}

impl EncryptApfsVolume {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        distribution: Distribution,
        disk: impl AsRef<Path>,
        name: impl AsRef<str>,
        planned_create_apfs_volume: &StatefulAction<CreateApfsVolume>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let name = name.as_ref().to_owned();
        let disk = disk.as_ref().to_path_buf();

        let mut command = Command::new("/usr/bin/security");
        command.args(["find-generic-password", "-a"]);
        command.arg(&name);
        command.arg("-s");
        command.arg(KEYCHAIN_NIX_STORE_SERVICE);
        command.arg("-l");
        command.arg(format!("{} encryption password", disk.display()));
        command.arg("-D");
        command.arg("Encrypted volume password");
        command.process_group(0);
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
        if command
            .status()
            .await
            .map_err(|e| Self::error(ActionErrorKind::command(&command, e)))?
            .success()
        {
            // The user has a password matching what we would create.
            if planned_create_apfs_volume.state == ActionState::Completed {
                // We detected a created volume already, and a password exists, so we can keep using that and skip doing anything
                return Ok(StatefulAction::completed(Self {
                    distribution,
                    name,
                    disk,
                }));
            }

            // Ask the user to remove it
            return Err(Self::error(EncryptApfsVolumeError::ExistingPasswordFound(
                name, disk,
            )));
        } else if planned_create_apfs_volume.state == ActionState::Completed {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "PascalCase")]
            struct DiskUtilDiskInfoOutput {
                file_vault: bool,
            }

            let output =
                execute_command(Command::new("/usr/sbin/diskutil").args(["info", "-plist", &name]))
                    .await
                    .map_err(Self::error)?;

            let parsed: DiskUtilDiskInfoOutput =
                plist::from_bytes(&output.stdout).map_err(Self::error)?;

            // The user has an already-encrypted volume, but we couldn't retrieve the password.
            // We won't be able to decrypt the volume.
            if parsed.file_vault {
                return Err(Self::error(
                    EncryptApfsVolumeError::MissingPasswordForExistingVolume(name, disk),
                ));
            }
        }

        // Ensure if the disk already exists, that it's encrypted
        let output =
            execute_command(Command::new("/usr/sbin/diskutil").args(["apfs", "list", "-plist"]))
                .await
                .map_err(Self::error)?;

        let parsed: DiskUtilApfsListOutput =
            plist::from_bytes(&output.stdout).map_err(Self::error)?;
        for container in parsed.containers {
            for volume in container.volumes {
                if volume.name.as_ref() == Some(&name) && volume.file_vault.unwrap_or(false) {
                    return Ok(StatefulAction::completed(Self {
                        distribution,
                        disk,
                        name,
                    }));
                }
            }
        }

        Ok(StatefulAction::uncompleted(Self {
            distribution,
            name,
            disk,
        }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "encrypt_apfs_volume")]
impl Action for EncryptApfsVolume {
    fn action_tag() -> ActionTag {
        ActionTag("encrypt_apfs_volume")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Encrypt volume `{}` on disk `{}`",
            self.name,
            self.disk.display()
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "encrypt_apfs_volume",
            disk = tracing::field::display(self.disk.display()),
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        disk = %self.disk.display(),
    ))]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // Generate a random password.
        let password: String = {
            const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                abcdefghijklmnopqrstuvwxyz\
                                    0123456789)(*&^%$#@!~";
            const PASSWORD_LEN: usize = 32;
            let mut rng = rand::thread_rng();

            (0..PASSWORD_LEN)
                .map(|_| {
                    let idx = rng.gen_range(0..CHARSET.len());
                    CHARSET[idx] as char
                })
                .collect()
        };

        let disk_str = &self.disk.to_str().expect("Could not turn disk into string"); /* Should not reasonably ever fail */

        let mut retry_tokens: usize = 60;
        loop {
            let mut command = Command::new("/usr/sbin/diskutil");
            command.process_group(0);
            command.args(["mount", &self.name]);
            command.stdin(std::process::Stdio::null());
            tracing::debug!(%retry_tokens, command = ?command.as_std(), "Waiting for volume mounting to succeed");

            let output = command
                .output()
                .await
                .map_err(|e| ActionErrorKind::command(&command, e))
                .map_err(Self::error)?;

            if output.status.success() {
                break;
            } else if retry_tokens == 0 {
                return Err(Self::error(ActionErrorKind::command_output(
                    &command, output,
                )))?;
            } else {
                retry_tokens = retry_tokens.saturating_sub(1);
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Add the password to the user keychain so they can unlock it later.
        let mut cmd = Command::new("/usr/bin/security");
        cmd.process_group(0).args([
            "add-generic-password",
            "-a",
            self.name.as_str(),
            "-s",
            KEYCHAIN_NIX_STORE_SERVICE,
            "-l",
            format!("{} encryption password", disk_str).as_str(),
            "-D",
            "Encrypted volume password",
            "-j",
            format!("Added automatically by the Nix installer for use by {NIX_VOLUME_MOUNTD_DEST}")
                .as_str(),
            "-w",
            password.as_str(),
            "-T",
            "/System/Library/CoreServices/APFSUserAgent",
            "-T",
            "/System/Library/CoreServices/CSUserAgent",
            "-T",
            "/usr/bin/security",
        ]);

        if self.distribution == Distribution::DeterminateNix {
            cmd.args(["-T", "/usr/local/bin/determinate-nixd"]);
        }

        cmd.arg("/Library/Keychains/System.keychain");

        // Add the password to the user keychain so they can unlock it later.
        execute_command(&mut cmd).await.map_err(Self::error)?;

        // Encrypt the mounted volume
        {
            let mut command = Command::new("/usr/sbin/diskutil");
            command.process_group(0);
            command.args([
                "apfs",
                "encryptVolume",
                self.name.as_str(),
                "-user",
                "disk",
                "-stdinpassphrase",
            ]);
            command.stdin(Stdio::piped());
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());
            tracing::trace!(command = ?command.as_std(), "Executing");
            let mut child = command
                .spawn()
                .map_err(|e| ActionErrorKind::command(&command, e))
                .map_err(Self::error)?;
            let mut stdin = child
                .stdin
                .take()
                .expect("child should have had a stdin handle");
            stdin
                .write_all(password.as_bytes())
                .await
                .map_err(|e| ActionErrorKind::Write("/dev/stdin".into(), e))
                .map_err(Self::error)?;
            stdin
                .write(b"\n")
                .await
                .map_err(|e| ActionErrorKind::Write("/dev/stdin".into(), e))
                .map_err(Self::error)?;
            let output = child
                .wait_with_output()
                .await
                .map_err(|e| ActionErrorKind::command(&command, e))
                .map_err(Self::error)?;
            match output.status.success() {
                true => {
                    tracing::trace!(
                        command = ?command.as_std(),
                        stderr = %String::from_utf8_lossy(&output.stderr),
                        stdout = %String::from_utf8_lossy(&output.stdout),
                        "Command success"
                    );
                },
                false => Err(Self::error(ActionErrorKind::command_output(
                    &command, output,
                )))?,
            }
        }

        execute_command(
            Command::new("/usr/sbin/diskutil")
                .process_group(0)
                .arg("unmount")
                .arg("force")
                .arg(&self.name),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!(
                "Remove encryption keys for volume `{}`",
                self.disk.display()
            ),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        disk = %self.disk.display(),
    ))]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let disk_str = self.disk.to_str().expect("Could not turn disk into string"); /* Should not reasonably ever fail */

        // TODO: This seems very rough and unsafe
        execute_command(
            Command::new("/usr/bin/security").process_group(0).args([
                "delete-generic-password",
                "-a",
                self.name.as_str(),
                "-s",
                KEYCHAIN_NIX_STORE_SERVICE,
                "-l",
                format!("{} encryption password", disk_str).as_str(),
                "-D",
                "Encrypted volume password",
                "-j",
                format!(
                    "Added automatically by the Nix installer for use by {NIX_VOLUME_MOUNTD_DEST}"
                )
                .as_str(),
            ]),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum EncryptApfsVolumeError {
    #[error("The keychain has an existing password for a non-existing \"{0}\" volume on disk `{1}`, consider removing the password with `sudo security delete-generic-password  -a \"{0}\" -s \"Nix Store\" -l \"{1} encryption password\" -D \"Encrypted volume password\"`. Note that it's possible to have several passwords stored, so you may need to run this command several times until receiving the message `The specified item could not be found in the keychain.`")]
    ExistingPasswordFound(String, PathBuf),
    #[error("The keychain lacks a password for the already existing \"{0}\" volume on disk `{1}`, consider removing the volume with `diskutil apfs deleteVolume \"{0}\"` (if you receive error -69888, you may need to run `sudo launchctl bootout system/org.nixos.darwin-store` and `sudo launchctl bootout system/org.nixos.nix-daemon` first)")]
    MissingPasswordForExistingVolume(String, PathBuf),
    #[error("The existing APFS volume \"{0}\" on disk `{1}` is not encrypted but it should be, consider removing the volume with `diskutil apfs deleteVolume \"{0}\"` (if you receive error -69888, you may need to run `sudo launchctl bootout system/org.nixos.darwin-store` and `sudo launchctl bootout system/org.nixos.nix-daemon` first)")]
    ExistingVolumeNotEncrypted(String, PathBuf),
}

impl From<EncryptApfsVolumeError> for ActionErrorKind {
    fn from(val: EncryptApfsVolumeError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}
