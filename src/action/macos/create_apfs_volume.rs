use std::path::{Path, PathBuf};
use std::time::Duration;

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
}

impl CreateApfsVolume {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: String,
        case_sensitive: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let output =
            execute_command(Command::new("/usr/sbin/diskutil").args(["apfs", "list", "-plist"]))
                .await
                .map_err(Self::error)?;

        let parsed: DiskUtilApfsListOutput =
            plist::from_bytes(&output.stdout).map_err(Self::error)?;
        for container in parsed.containers {
            for volume in container.volumes {
                if volume.name.as_ref() == Some(&name) {
                    return Ok(StatefulAction::completed(Self {
                        disk: disk.as_ref().to_path_buf(),
                        name,
                        case_sensitive,
                    }));
                }
            }
        }

        Ok(StatefulAction::uncompleted(Self {
            disk: disk.as_ref().to_path_buf(),
            name,
            case_sensitive,
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
        let Self {
            disk,
            name,
            case_sensitive,
        } = self;

        execute_command(
            Command::new("/usr/sbin/diskutil")
                .process_group(0)
                .args([
                    "apfs",
                    "addVolume",
                    &format!("{}", disk.display()),
                    if !*case_sensitive {
                        "APFS"
                    } else {
                        "Case-sensitive APFS"
                    },
                    name,
                    "-nomount",
                ])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

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
