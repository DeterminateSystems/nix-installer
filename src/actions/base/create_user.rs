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
            action_state: ActionState::Uncompleted,
        }
    }
}

#[async_trait::async_trait]
impl Actionable for CreateUser {
    type Error = CreateUserError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            let Self {
                name,
                uid,
                gid,
                action_state: _,
            } = self;

            vec![ActionDescription::new(
                format!("Create user {name} with UID {uid} with group {gid}"),
                vec![format!(
                    "The nix daemon requires system users it can act as in order to build"
                )],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        user = self.name,
        uid = self.uid,
        gid = self.gid,
    ))]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            name,
            uid,
            gid,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating user");
            return Ok(());
        }
        tracing::debug!("Creating user");

        use target_lexicon::OperatingSystem;
        match target_lexicon::OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                execute_command(Command::new("/usr/bin/dscl").args([
                    ".",
                    "create",
                    &format!("/Users/{name}"),
                    "UniqueId",
                    &format!("{uid}"),
                    "PrimaryGroupID",
                    &format!("{gid}"),
                ]))
                .await
                .map_err(Self::Error::Command)?;
            },
            _ => {
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
            },
        }

        tracing::trace!("Created user");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            let Self {
                name,
                uid,
                gid,
                action_state: _,
            } = self;

            vec![ActionDescription::new(
                format!("Delete user {name} with UID {uid} with group {gid}"),
                vec![format!(
                    "The nix daemon requires system users it can act as in order to build"
                )],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        user = self.name,
        uid = self.uid,
        gid = self.gid,
    ))]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            name,
            uid: _,
            gid: _,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already completed: Deleting user");
            return Ok(());
        }
        tracing::debug!("Deleting user");

        use target_lexicon::OperatingSystem;
        match target_lexicon::OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                todo!()
            },
            _ => {
                execute_command(Command::new("userdel").args([&name.to_string()]))
                    .await
                    .map_err(Self::Error::Command)?;
            },
        };

        tracing::trace!("Deleted user");
        *action_state = ActionState::Uncompleted;
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
