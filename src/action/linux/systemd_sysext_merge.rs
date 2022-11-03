use std::path::PathBuf;

use tokio::process::Command;

use crate::execute_command;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SystemdSysextMerge {
    action_state: ActionState,
}

impl SystemdSysextMerge {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<Self, SystemdSysextMergeError> {
        Ok(Self {
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "systemd_sysext_merge")]
impl Action for SystemdSysextMerge {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self { action_state } = self;
        if *action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Run `systemd-sysext refresh`"),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields())]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { action_state } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Merging systemd-sysext");
            return Ok(());
        }
        tracing::debug!("Merging systemd-sysext");

        execute_command(Command::new("systemd-sysext").arg("refresh"))
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

    #[tracing::instrument(skip_all, fields())]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { action_state } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Stopping systemd unit");
            return Ok(());
        }
        tracing::debug!("Unmerging systemd-sysext");

        // TODO(@Hoverbear): Handle proxy vars
        execute_command(Command::new("systemd-sysext").arg("refresh"))
            .await
            .map_err(|e| SystemdSysextMergeError::Command(e).boxed())?;

        tracing::trace!("Unmerged systemd-sysext");
        *action_state = ActionState::Completed;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SystemdSysextMergeError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
