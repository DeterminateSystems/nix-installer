use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};

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
        let output = execute_command(Command::new("/usr/sbin/diskutil").args(["apfs", "list"]))
            .await
            .map_err(ActionError::Command)?;

        let output_string = String::from_utf8(output.stdout)?;
        for line in output_string.lines() {
            if line.contains("Name:") && line.contains(&name) {
                return Err(ActionError::Custom(Box::new(
                    CreateApfsVolumeError::ExistingVolume(name),
                )));
            }
        }

        Ok(Self {
            disk: disk.as_ref().to_path_buf(),
            name,
            case_sensitive,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_volume")]
impl Action for CreateApfsVolume {
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
        .map_err(|e| ActionError::Command(e))?;

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
        let Self {
            disk: _,
            name,
            case_sensitive: _,
        } = self;

        execute_command(
            Command::new("/usr/sbin/diskutil")
                .process_group(0)
                .args(["apfs", "deleteVolume", name])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| ActionError::Command(e))?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateApfsVolumeError {
    #[error("Existing volume called `{0}` found in `diskutil apfs list`, delete it with `diskutil apfs deleteVolume \"{0}\"`")]
    ExistingVolume(String),
}
