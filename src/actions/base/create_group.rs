use serde::Serialize;
use tokio::process::Command;

use crate::execute_command;

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateGroup {
    name: String,
    gid: usize,
    action_state: ActionState,
}

impl CreateGroup {
    #[tracing::instrument(skip_all)]
    pub fn plan(name: String, gid: usize) -> Self {
        Self {
            name,
            gid,
            action_state: ActionState::Planned,
        }
    }
}

#[async_trait::async_trait]
impl Actionable for CreateGroup {
    type Error = CreateGroupError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            name,
            gid,
            action_state: _,
        } = &self;
        vec![ActionDescription::new(
            format!("Create group {name} with GID {gid}"),
            vec![format!(
                "The nix daemon requires a system user group its system users can be part of"
            )],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            name,
            gid,
            action_state,
        } = self;

        execute_command(Command::new("groupadd").args(["-g", &gid.to_string(), "--system", &name]))
            .await
            .map_err(CreateGroupError::Command)?;

        *action_state = ActionState::Completed;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            name,
            gid: _,
            action_state,
        } = self;

        execute_command(Command::new("groupdel").arg(&name))
            .await
            .map_err(CreateGroupError::Command)?;

        *action_state = ActionState::Reverted;
        Ok(())
    }
}

impl From<CreateGroup> for Action {
    fn from(v: CreateGroup) -> Self {
        Action::CreateGroup(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateGroupError {
    #[error("Failed to execute command")]
    Command(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
