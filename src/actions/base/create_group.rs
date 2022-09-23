use serde::Serialize;
use tokio::process::Command;

use crate::{HarmonicError, execute_command};

use crate::actions::{ActionDescription, Actionable, ActionState, Action};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateGroup {
    name: String,
    gid: usize,
}

impl CreateGroup {
    #[tracing::instrument(skip_all)]
    pub fn plan(name: String, gid: usize) -> Self {
        Self { name, gid }
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<CreateGroup> {
    type Error = CreateOrAppendFileError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { name, gid } = &self;
        vec![ActionDescription::new(
            format!("Create group {name} with GID {gid}"),
            vec![format!(
                "The nix daemon requires a system user group its system users can be part of"
            )],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self { name, gid } = self;

        execute_command(
            Command::new("groupadd").args(["-g", &gid.to_string(), "--system", &name]),
            false,
        ).await?;

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<CreateGroup>> for ActionState<Action> {
    fn from(v: ActionState<CreateGroup>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::CreateGroup(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::CreateGroup(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::CreateGroup(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateGroupError {
}