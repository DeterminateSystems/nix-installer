use crate::execute_command;
use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};
use std::path::PathBuf;
use tokio::process::Command;

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
#[typetag::serde(name = "systemd_sysext_merge")]
impl Action for SystemdSysextMerge {
    fn tracing_synopsis(&self) -> String {
        format!("Run `systemd-sysext merge `{}`", device.display())
    }

    fn describe_execute(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        device = %self.device.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            device,
            action_state,
        } = self;

        execute_command(
            Command::new("systemd-sysext")
                .process_group(0)
                .arg("merge")
                .arg(device)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| SystemdSysextMergeError::Command(e).boxed())?;

        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
                "Stop the systemd Nix service and socket".to_string(),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            )]
    }

    #[tracing::instrument(skip_all, fields(
        device = %self.device.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            device,
            action_state,
        } = self;

        // TODO(@Hoverbear): Handle proxy vars
        execute_command(
            Command::new("systemd-sysext")
                .process_group(0)
                .arg("unmerge")
                .arg(device)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| SystemdSysextMergeError::Command(e).boxed())?;

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
pub enum SystemdSysextMergeError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
