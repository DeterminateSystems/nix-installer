use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::Command;

use crate::execute_command;

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct BootstrapVolume {
    path: PathBuf,
    action_state: ActionState,
}

impl BootstrapVolume {
    #[tracing::instrument(skip_all)]
    pub async fn plan(path: impl AsRef<Path>) -> Result<Self, BootstrapVolumeError> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for BootstrapVolume {
    type Error = BootstrapVolumeError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Bootstrap and kickstart `{}`", self.path.display()),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self { path, action_state } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Bootstrapping volume");
            return Ok(());
        }
        tracing::debug!("Bootstrapping volume");

        execute_command(
            Command::new("launchctl")
                .args(["bootstrap", "system"])
                .arg(path),
        )
        .await
        .map_err(Self::Error::Command)?;
        execute_command(Command::new("launchctl").args([
            "kickstart",
            "-k",
            "system/org.nixos.darwin-store",
        ]))
        .await
        .map_err(Self::Error::Command)?;

        tracing::trace!("Bootstrapped volume");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Stop `{}`", self.path.display()),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self { path, action_state } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Stop volume");
            return Ok(());
        }
        tracing::debug!("Stop volume");

        execute_command(
            Command::new("launchctl")
                .args(["bootout", "system"])
                .arg(path),
        )
        .await
        .map_err(Self::Error::Command)?;

        tracing::trace!("Stopped volume");
        *action_state = ActionState::Completed;
        Ok(())
    }
}

impl From<BootstrapVolume> for Action {
    fn from(v: BootstrapVolume) -> Self {
        Action::DarwinBootstrapVolume(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum BootstrapVolumeError {
    #[error("Failed to execute command")]
    Command(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
