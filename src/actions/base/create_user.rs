use serde::Serialize;
use tokio::process::Command;

use crate::{HarmonicError, execute_command};

use crate::actions::{ActionDescription, Actionable, ActionState, Action};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUser {
    name: String,
    uid: usize,
    gid: usize,
}

impl CreateUser {
    #[tracing::instrument(skip_all)]
    pub fn plan(name: String, uid: usize, gid: usize) -> Self {
        Self { name, uid, gid }
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<CreateUser> {
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
        let Self { name, uid, gid } = self;

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
        ]), false).await?;

        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<CreateUser>> for ActionState<Action> {
    fn from(v: ActionState<CreateUser>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::CreateUser(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::CreateUser(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::CreateUser(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateUserError {

}
