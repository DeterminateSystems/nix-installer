use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use rand::Rng as _;
use tokio::io::AsyncWriteExt as _;
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};
use crate::os::darwin::{DiskUtilApfsListOutput, DiskUtilInfoOutput};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_apfs_volume")]
pub struct CreateApfsVolume {
    disk: PathBuf,
    name: String,
    case_sensitive: bool,
    determinate_nix: bool,
}

impl CreateApfsVolume {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: String,
        case_sensitive: bool,
        determinate_nix: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let disk = disk.as_ref().to_path_buf();

        let output =
            execute_command(Command::new("/usr/sbin/diskutil").args(["apfs", "list", "-plist"]))
                .await
                .map_err(Self::error)?;

        let parsed: DiskUtilApfsListOutput =
            plist::from_bytes(&output.stdout).map_err(Self::error)?;
        for container in parsed.containers {
            for volume in container.volumes {
                if volume.name.as_ref() == Some(&name) {
                    if volume.file_vault {
                        todo!("edge-case I'll think about later");
                        return Ok(StatefulAction::completed(Self {
                            disk,
                            name,
                            case_sensitive,
                            determinate_nix,
                        }));
                    }
                }
            }
        }

        Ok(StatefulAction::uncompleted(Self {
            disk,
            name,
            case_sensitive,
            determinate_nix,
        }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_apfs_volume")]
impl Action for CreateApfsVolume {
    fn action_tag() -> ActionTag {
        ActionTag("create_apfs_volume")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create an APFS volume on `{}` named `{}`",
            self.disk.display(),
            self.name
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "create_volume",
            disk = %self.disk.display(),
            name = %self.name,
            case_sensitive = %self.case_sensitive,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
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

        let disk_str = self.disk.display().to_string();

        {
            // Add the password to the user keychain so they can unlock it later.
            let mut command = Command::new("/usr/bin/security");
            command.process_group(0).args([
                "add-generic-password",
                "-a",
                self.name.as_str(),
                "-s",
                "Nix Store",
                "-l",
                format!("{} encryption password", disk_str).as_str(),
                "-D",
                "Encrypted volume password",
                "-j",
                format!(
                    "Added automatically by the Nix installer for use by {{NIX_VOLUME_MOUNTD_DEST}}"
                )
                .as_str(),
                "-T",
                "/System/Library/CoreServices/APFSUserAgent",
                "-T",
                "/System/Library/CoreServices/CSUserAgent",
                "-T",
                "/usr/bin/security",
            ]);

            if self.determinate_nix {
                command.args(["-T", "/usr/local/bin/determinate-nixd"]);
            }

            command.arg("-w"); // "Specify -w as the last option to be prompted."
            command.arg("/Library/Keychains/System.keychain");

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

        // Encrypt the mounted volume
        {
            let mut command = Command::new("/usr/sbin/diskutil");
            command.process_group(0);
            command.args([
                "apfs",
                "addVolume",
                &disk_str,
                if !self.case_sensitive {
                    "APFS"
                } else {
                    "Case-sensitive APFS"
                },
                &self.name,
                "-stdinpassphrase",
                "-mountpoint",
                "/nix",
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

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!(
                "Remove the volume on `{}` named `{}`",
                self.disk.display(),
                self.name
            ),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let currently_mounted = {
            let the_plist = DiskUtilInfoOutput::for_volume_name(&self.name)
                .await
                .map_err(Self::error)?;
            the_plist.is_mounted()
        };

        // Unmounts the volume before attempting to remove it, avoiding 'in use' errors
        // https://github.com/DeterminateSystems/nix-installer/issues/647
        if currently_mounted {
            execute_command(
                Command::new("/usr/sbin/diskutil")
                    .process_group(0)
                    .args(["unmount", "force", &self.name])
                    .stdin(std::process::Stdio::null()),
            )
            .await
            .map_err(Self::error)?;
        } else {
            tracing::debug!("Volume was already unmounted, can skip unmounting")
        }

        // NOTE(cole-h): We believe that, because we're running the unmount force -> deleteVolume
        // commands in an automated fashion, there's a race condition where we're running them too
        // close to each other, so the OS doesn't notice the volume has been unmounted / hasn't
        // completed its "unmount the volume" tasks by the time we try to delete it. If that is the
        // case (unfortunately, we have been unable to reproduce this issue on the machines we have
        // access to!), then trying to delete the volume 10 times -- with 500ms of time between
        // attempts -- should alleviate this.
        // https://github.com/DeterminateSystems/nix-installer/issues/1303
        // https://github.com/DeterminateSystems/nix-installer/issues/1267
        // https://github.com/DeterminateSystems/nix-installer/issues/1085
        let mut retry_tokens: usize = 10;
        loop {
            let mut command = Command::new("/usr/sbin/diskutil");
            command.process_group(0);
            command.args(["apfs", "deleteVolume", &self.name]);
            command.stdin(std::process::Stdio::null());
            tracing::debug!(%retry_tokens, command = ?command.as_std(), "Waiting for volume deletion to succeed");

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

        Ok(())
    }
}
