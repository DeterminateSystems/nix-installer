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
    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Start the systemd Nix service and socket".to_string(),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
        name = %self.name,
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk: _,
            name,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Unmounting volume");
            return Ok(());
        }
        tracing::debug!("Unmounting volume");

        execute_command(
            Command::new("/usr/sbin/diskutil")
                .args(["unmount", "force"])
                .arg(name),
        )
        .await
        .map_err(|e| UnmountVolumeError::Command(e).boxed())?;

        tracing::trace!("Unmounted volume");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Stop the systemd Nix service and socket".to_string(),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
        name = %self.name,
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk: _,
            name,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Stopping systemd unit");
            return Ok(());
        }
        tracing::debug!("Stopping systemd unit");

        execute_command(
            Command::new(" /usr/sbin/diskutil")
                .args(["unmount", "force"])
                .arg(name),
        )
        .await
        .map_err(|e| UnmountVolumeError::Command(e).boxed())?;

        tracing::trace!("Stopped systemd unit");
        *action_state = ActionState::Completed;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UnmountVolumeError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
