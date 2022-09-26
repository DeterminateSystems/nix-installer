use serde::Serialize;
use tokio::process::Command;

use crate::execute_command;

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUser {
    name: String,
    uid: usize,
    gid: usize,
    action_state: ActionState,
}

impl CreateUser {
    #[tracing::instrument(skip_all)]
    pub fn plan(name: String, uid: usize, gid: usize) -> Self {
        Self {
            name,
            uid,
            gid,
            action_state: ActionState::Planned,
        }
    }
}

#[async_trait::async_trait]
impl Actionable for CreateUser {
    type Error = CreateUserError;
    fn description(&self) -> Vec<ActionDescription> {
        let name = &self.name;
        let uid = &self.uid;
        vec![ActionDescription::new(
            format!("Create user {name} with UID {uid}"),
            vec![format!(
                "The nix daemon requires system users it can act as in order to build"
            )],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            name,
            uid,
            gid,
            action_state,
        } = self;

        execute_command(Command::new("useradd").args([
            "--home-dir",
            "/var/empty",
            "--comment",
            &format!("\"Nix build user\""),
            "--gid",
            &gid.to_string(),
            "--groups",
            &gid.to_string(),
            "--no-user-group",
            "--system",
            "--shell",
            "/sbin/nologin",
            "--uid",
            &uid.to_string(),
            "--password",
            "\"!\"",
            &name.to_string(),
        ]))
        .await
        .map_err(Self::Error::Command)?;

        *action_state = ActionState::Completed;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            name,
            uid: _,
            gid: _,
            action_state,
        } = self;

        execute_command(Command::new("userdel").args([&name.to_string()]))
            .await
            .map_err(Self::Error::Command)?;

        *action_state = ActionState::Completed;
        Ok(())
    }
}

impl From<CreateUser> for Action {
    fn from(v: CreateUser) -> Self {
        Action::CreateUser(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateUserError {
    #[error("Failed to execute command")]
    Command(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
