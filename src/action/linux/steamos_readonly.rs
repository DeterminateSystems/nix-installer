use crate::execute_command;
use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};
use std::path::PathBuf;
use tokio::process::Command;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SteamosReadonly {
    read_only: bool,
    action_state: ActionState,
}

impl SteamosReadonly {
    #[tracing::instrument(skip_all)]
    pub async fn plan(read_only: bool) -> Result<Self, SteamosReadonlyError> {
        Ok(Self {
            read_only,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "steamos_readonly")]
impl Action for SteamosReadonly {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            action_state,
            read_only,
        } = self;
        if *action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!(
                    "Run `steamos-readonly {}` so a `/nix` stub can be created",
                    if *read_only { "enable" } else { "disable" },
                ),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        read_only = %self.read_only,
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            read_only,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Running `steamos-readonly`");
            return Ok(());
        }
        tracing::debug!("Running `steamos-readonly`");

        execute_command(Command::new("steamos-readonly").arg(if *read_only {
            "enable"
        } else {
            "disable"
        }))
        .await
        .map_err(|e| SteamosReadonlyError::Command(e).boxed())?;

        tracing::trace!("Ran `steamos-readonly`");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!(
                    "Run `steamos-readonly {}` so a `/nix` stub can be created",
                    if self.read_only { "disable" } else { "enable" },
                ),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        read_only = %self.read_only,
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            read_only,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Stopping systemd unit");
            return Ok(());
        }
        tracing::debug!("Running `steamos-readonly`");

        execute_command(Command::new("steamos-readonly").arg(if *read_only {
            "disable"
        } else {
            "enable"
        }))
        .await
        .map_err(|e| SteamosReadonlyError::Command(e).boxed())?;

        tracing::trace!("Ran `steamos-readonly`");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn action_state(&self) -> ActionState {
        self.action_state
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SteamosReadonlyError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
