use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::execute_command;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct UnmountVolume {
    disk: PathBuf,
    name: String,
    action_state: ActionState,
}

impl UnmountVolume {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: String,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let disk = disk.as_ref().to_owned();
        Ok(Self {
            disk,
            name,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "unmount_volume")]
impl Action for UnmountVolume {
    fn tracing_synopsis(&self) -> String {
        format!("Unmount the `{}` volume", self.name)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
        name = %self.name,
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk: _,
            name,
            action_state: _,
        } = self;

        execute_command(
            Command::new("/usr/sbin/diskutil")
                .args(["unmount", "force"])
                .arg(name)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| UnmountVolumeError::Command(e).boxed())?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
        name = %self.name,
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk: _,
            name,
            action_state: _,
        } = self;

        execute_command(
            Command::new("/usr/sbin/diskutil")
                .args(["unmount", "force"])
                .arg(name)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| UnmountVolumeError::Command(e).boxed())?;

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
pub enum UnmountVolumeError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
