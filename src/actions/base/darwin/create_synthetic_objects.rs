use serde::Serialize;
use tokio::process::Command;

use crate::execute_command;

use crate::actions::{ActionDescription, ActionError, ActionState, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateSyntheticObjects {
    action_state: ActionState,
}

impl CreateSyntheticObjects {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create-synthetic-objects")]
impl Actionable for CreateSyntheticObjects {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Create objects defined in `/etc/synthetic.conf`".to_string(),
                vec!["Populates the `/nix` path".to_string()],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields())]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { action_state } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating synthetic objects");
            return Ok(());
        }
        tracing::debug!("Creating synthetic objects");

        // Yup we literally call both and ignore the error! Reasoning: https://github.com/NixOS/nix/blob/95331cb9c99151cbd790ceb6ddaf49fc1c0da4b3/scripts/create-darwin-volume.sh#L261
        execute_command(
            Command::new("/System/Library/Filesystems/apfs.fs/Contents/Resources/apfs.util")
                .arg("-t"),
        )
        .await
        .ok(); // Deliberate
        execute_command(
            Command::new("/System/Library/Filesystems/apfs.fs/Contents/Resources/apfs.util")
                .arg("-B"),
        )
        .await
        .ok(); // Deliberate

        tracing::trace!("Created synthetic objects");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Refresh the objects defined in `/etc/synthetic.conf`".to_string(),
                vec!["Will remove the `/nix` path".to_string()],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields())]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { action_state } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Refreshing synthetic objects");
            return Ok(());
        }
        tracing::debug!("Refreshing synthetic objects");

        // Yup we literally call both and ignore the error! Reasoning: https://github.com/NixOS/nix/blob/95331cb9c99151cbd790ceb6ddaf49fc1c0da4b3/scripts/create-darwin-volume.sh#L261
        execute_command(
            Command::new("/System/Library/Filesystems/apfs.fs/Contents/Resources/apfs.util")
                .arg("-t"),
        )
        .await
        .ok(); // Deliberate
        execute_command(
            Command::new("/System/Library/Filesystems/apfs.fs/Contents/Resources/apfs.util")
                .arg("-B"),
        )
        .await
        .ok(); // Deliberate

        tracing::trace!("Refreshed synthetic objects");
        *action_state = ActionState::Completed;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateSyntheticObjectsError {
    #[error("Failed to execute command")]
    Command(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
