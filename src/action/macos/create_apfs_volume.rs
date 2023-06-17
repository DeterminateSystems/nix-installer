use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};
use crate::os::darwin::DiskUtilApfsListOutput;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
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
#[typetag::serde(name = "create_volume")]
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
        execute_command(
            Command::new("/usr/sbin/diskutil")
                .process_group(0)
                .args(["apfs", "deleteVolume", &self.name])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }
}
