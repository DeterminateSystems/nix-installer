use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::execute_command;

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct EncryptVolume {
    disk: PathBuf,
    password: String,
    action_state: ActionState,
}

impl EncryptVolume {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        password: String,
    ) -> Result<Self, EncryptVolumeError> {
        Ok(Self {
            disk: disk.as_ref().to_path_buf(),
            password,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for EncryptVolume {
    type Error = EncryptVolumeError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Encrypt volume `{}`", self.disk.display()),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            disk,
            password,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Encrypting volume");
            return Ok(());
        }
        tracing::debug!("Encrypting volume");

        todo!();

        tracing::trace!("Encrypted volume");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![]
        }
    }

    #[tracing::instrument(skip_all, fields(
        disk = %self.disk.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            disk,
            password,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Unencrypted volume (noop)");
            return Ok(());
        }
        tracing::debug!("Unencrypted volume (noop)");

        tracing::trace!("Unencrypted volume (noop)");
        *action_state = ActionState::Completed;
        Ok(())
    }
}

impl From<EncryptVolume> for Action {
    fn from(v: EncryptVolume) -> Self {
        Action::DarwinEncryptVolume(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum EncryptVolumeError {
    #[error("Failed to execute command")]
    Command(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
