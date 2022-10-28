use std::path::{Path, PathBuf};

use crate::action::{Action, ActionDescription, ActionState};

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
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            disk: disk.as_ref().to_path_buf(),
            password,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "encrypt-volume")]
impl Action for EncryptVolume {
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
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk: _,
            password: _,
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
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            disk: _,
            password: _,
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

#[derive(Debug, thiserror::Error)]
pub enum EncryptVolumeError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
