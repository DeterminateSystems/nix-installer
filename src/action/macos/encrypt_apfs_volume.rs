use crate::{
    action::{
        macos::NIX_VOLUME_MOUNTD_DEST, Action, ActionDescription, ActionError, StatefulAction,
    },
    execute_command,
};
use rand::Rng;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tracing::{span, Span};

/**
Encrypt an APFS volume
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct EncryptApfsVolume {
    disk: PathBuf,
    name: String,
}

impl EncryptApfsVolume {
    pub fn typetag() -> &'static str {
        "encrypt_apfs_volume"
    }
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: impl AsRef<str>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let name = name.as_ref().to_owned();
        Ok(Self {
            name,
            disk: disk.as_ref().to_path_buf(),
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "encrypt_volume")]
impl Action for EncryptApfsVolume {
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
            "encrypt_volume",
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
        let Self { disk, name } = self;

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
            Command::new("/usr/bin/security").process_group(0).args([
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
        execute_command(Command::new("/usr/sbin/diskutil").process_group(0).args([
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
                .process_group(0)
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

    #[tracing::instrument(level = "debug", skip_all, fields(
        disk = %self.disk.display(),
    ))]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self { disk, name } = self;

        let disk_str = disk.to_str().expect("Could not turn disk into string"); /* Should not reasonably ever fail */

        // TODO: This seems very rough and unsafe
        execute_command(
            Command::new("/usr/bin/security").process_group(0).args([
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
}
