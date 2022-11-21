use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::execute_command;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct BootstrapVolume {
    path: PathBuf,
    action_state: ActionState,
}

impl BootstrapVolume {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "bootstrap_volume")]
impl Action for BootstrapVolume {
    fn tracing_synopsis(&self) -> String {
        format!("Bootstrap and kickstart `{}`", self.path.display())
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            path,
            action_state: _,
        } = self;

        execute_command(
            Command::new("launchctl")
                .args(["bootstrap", "system"])
                .arg(path)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| BootstrapVolumeError::Command(e).boxed())?;
        execute_command(
            Command::new("launchctl")
                .args(["kickstart", "-k", "system/org.nixos.darwin-store"])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| BootstrapVolumeError::Command(e).boxed())?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Stop `{}`", self.path.display()),
            vec![],
        )]
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            path,
            action_state: _,
        } = self;

        execute_command(
            Command::new("launchctl")
                .args(["unload", "system/org.nixos.darwin-store"])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| BootstrapVolumeError::Command(e).boxed())?;

        execute_command(
            Command::new("launchctl")
                .args(["bootout", "system"])
                .arg(path)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| BootstrapVolumeError::Command(e).boxed())?;

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
pub enum BootstrapVolumeError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
