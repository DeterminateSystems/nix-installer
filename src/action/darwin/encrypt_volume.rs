use crate::{
    action::{darwin::NIX_VOLUME_MOUNTD_DEST, Action, ActionDescription, ActionState},
    execute_command,
};
use rand::Rng;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct EncryptVolume {
    disk: PathBuf,
    name: String,
    action_state: ActionState,
}

impl EncryptVolume {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: impl AsRef<str>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let name = name.as_ref().to_owned();
        Ok(Self {
            name,
            disk: disk.as_ref().to_path_buf(),
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "encrypt_volume")]
impl Action for EncryptVolume {
    fn tracing_synopsis(&self) -> String {
        format!(
            "Encrypt volume `{}` on disk `{}`",
            self.name,
            self.disk.display()
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk,
            name,
            action_state: _,
        } = self;

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

        let disk_str = disk.to_str().expect("Could not turn disk into string"); /* Should not reasonably ever fail */

        execute_command(Command::new("/usr/sbin/diskutil").arg("mount").arg(&name)).await?;

        // Add the password to the user keychain so they can unlock it later.
        execute_command(
            Command::new("/usr/bin/security").args([
                "add-generic-password",
                "-a",
                name.as_str(),
                "-s",
                name.as_str(),
                "-l",
                format!("{} encryption password", disk_str).as_str(),
                "-D",
                "Encrypted volume password",
                "-j",
                format!(
                    "Added automatically by the Nix installer for use by {NIX_VOLUME_MOUNTD_DEST}"
                )
                .as_str(),
                "-w",
                password.as_str(),
                "-T",
                "/System/Library/CoreServices/APFSUserAgent",
                "-T",
                "/System/Library/CoreServices/CSUserAgent",
                "-T",
                "/usr/bin/security",
                "/Library/Keychains/System.keychain",
            ]),
        )
        .await?;

        // Encrypt the mounted volume
        execute_command(Command::new("/usr/sbin/diskutil").args([
            "apfs",
            "encryptVolume",
            name.as_str(),
            "-user",
            "disk",
            "-passphrase",
            password.as_str(),
        ]))
        .await?;

        execute_command(
            Command::new("/usr/sbin/diskutil")
                .arg("unmount")
                .arg("force")
                .arg(&name),
        )
        .await?;

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

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk,
            name,
            action_state: _,
        } = self;

        let disk_str = disk.to_str().expect("Could not turn disk into string"); /* Should not reasonably ever fail */

        // TODO: This seems very rough and unsafe
        execute_command(
            Command::new("/usr/bin/security").args([
                "delete-generic-password",
                "-a",
                name.as_str(),
                "-s",
                name.as_str(),
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
        .await?;

        Ok(())
    }

    fn action_state(&self) -> ActionState {
        self.action_state
    }

    fn set_action_state(&mut self, action_state: ActionState) {
        self.action_state = action_state;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EncryptVolumeError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
