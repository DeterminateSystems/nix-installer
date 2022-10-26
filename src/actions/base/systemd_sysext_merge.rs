use std::path::PathBuf;

use serde::Serialize;
use tokio::process::Command;

use crate::execute_command;

use crate::actions::{ActionDescription, ActionError, ActionState, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SystemdSysextMerge {
    device: PathBuf,
    action_state: ActionState,
}

impl SystemdSysextMerge {
    #[tracing::instrument(skip_all)]
    pub async fn plan(device: PathBuf) -> Result<Self, SystemdSysextMergeError> {
        Ok(Self {
            device,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "systemd-sysext-merge")]
impl Actionable for SystemdSysextMerge {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            action_state,
            device,
        } = self;
        if *action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Run `systemd-sysext merge `{}`", device.display()),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        device = %self.device.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            device,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Merging systemd-sysext");
            return Ok(());
        }
        tracing::debug!("Merging systemd-sysext");

        execute_command(Command::new("systemd-sysext").arg("merge").arg(device))
            .await
            .map_err(|e| SystemdSysextMergeError::Command(e).boxed())?;

        tracing::trace!("Merged systemd-sysext");
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
        device = %self.device.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            device,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Stopping systemd unit");
            return Ok(());
        }
        tracing::debug!("Unmrging systemd-sysext");

        // TODO(@Hoverbear): Handle proxy vars
        execute_command(Command::new("systemd-sysext").arg("unmerge").arg(device))
            .await
            .map_err(|e| SystemdSysextMergeError::Command(e).boxed())?;

        tracing::trace!("Unmerged systemd-sysext");
        *action_state = ActionState::Completed;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum SystemdSysextMergeError {
    #[error("Failed to execute command")]
    Command(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
