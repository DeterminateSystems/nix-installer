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
            action_state: ActionState::Uncompleted,
        }
    }
}

#[async_trait::async_trait]
impl Actionable for CreateGroup {
    type Error = CreateGroupError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            name,
            gid,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Create group {name} with GID {gid}"),
                vec![format!(
                    "The nix daemon requires a system user group its system users can be part of"
                )],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        user = self.name,
        gid = self.gid,
    ))]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            name,
            gid,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating group");
            return Ok(());
        }
        tracing::debug!("Creating group");

        execute_command(Command::new("groupadd").args(["-g", &gid.to_string(), "--system", &name]))
            .await
            .map_err(CreateGroupError::Command)?;

        tracing::trace!("Created group");
        *action_state = ActionState::Completed;
        Ok(())
    }


    fn describe_revert(&self) -> Vec<ActionDescription> {
        let Self {
            name,
            gid: _,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Delete group {name}"),
                vec![format!(
                    "The nix daemon requires a system user group its system users can be part of"
                )],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        user = self.name,
        gid = self.gid,
    ))]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            name,
            gid: _,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Deleting group");
            return Ok(());
        }
        tracing::debug!("Deleting group");

        execute_command(Command::new("groupdel").arg(&name))
            .await
            .map_err(CreateGroupError::Command)?;

        tracing::trace!("Deleted group");
        *action_state = ActionState::Uncompleted;
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
