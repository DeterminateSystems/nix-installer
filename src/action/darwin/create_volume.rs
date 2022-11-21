use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::execute_command;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateVolume {
    disk: PathBuf,
    name: String,
    case_sensitive: bool,
    action_state: ActionState,
}

impl CreateVolume {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: String,
        case_sensitive: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            disk: disk.as_ref().to_path_buf(),
            name,
            case_sensitive,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_volume")]
impl Action for CreateVolume {
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create a volume on `{}` named `{}`",
            self.disk.display(),
            self.name
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
        name = %self.name,
        case_sensitive = %self.case_sensitive,
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk,
            name,
            case_sensitive,
            action_state,
        } = self;

        execute_command(
            Command::new("/usr/sbin/diskutil")
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
        .map_err(|e| CreateVolumeError::Command(e).boxed())?;

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

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
        name = %self.name,
        case_sensitive = %self.case_sensitive,
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk: _,
            name,
            case_sensitive: _,
            action_state,
        } = self;

        execute_command(
            Command::new("/usr/sbin/diskutil")
                .args(["apfs", "deleteVolume", name])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| CreateVolumeError::Command(e).boxed())?;

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
pub enum CreateVolumeError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
