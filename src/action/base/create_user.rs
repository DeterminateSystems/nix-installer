use tokio::process::Command;

use crate::execute_command;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUser {
    name: String,
    uid: usize,
    groupname: String,
    gid: usize,
    action_state: ActionState,
}

impl CreateUser {
    #[tracing::instrument(skip_all)]
    pub fn plan(name: String, uid: usize, groupname: String, gid: usize) -> Self {
        Self {
            name,
            uid,
            groupname,
            gid,
            action_state: ActionState::Uncompleted,
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_user")]
impl Action for CreateUser {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            let Self {
                name,
                uid,
                groupname: _,
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
        groupname = self.groupname,
        gid = self.gid,
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            name,
            uid,
            groupname,
            gid,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating user");
            return Ok(());
        }
        tracing::debug!("Creating user");

        use target_lexicon::OperatingSystem;
        match OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                // TODO(@hoverbear): Make this actually work...
                // Right now, our test machines do not have a secure token and cannot delete users.

                if Command::new("/usr/bin/dscl")
                    .args([".", "-read", &format!("/Users/{name}")])
                    .status()
                    .await?
                    .success()
                {
                    ()
                } else {
                    execute_command(Command::new("/usr/bin/dscl").args([
                        ".",
                        "-create",
                        &format!("/Users/{name}"),
                    ]))
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(Command::new("/usr/bin/dscl").args([
                        ".",
                        "-create",
                        &format!("/Users/{name}"),
                        "UniqueID",
                        &format!("{uid}"),
                    ]))
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(Command::new("/usr/bin/dscl").args([
                        ".",
                        "-create",
                        &format!("/Users/{name}"),
                        "PrimaryGroupID",
                        &format!("{gid}"),
                    ]))
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(Command::new("/usr/bin/dscl").args([
                        ".",
                        "-create",
                        &format!("/Users/{name}"),
                        "NFSHomeDirectory",
                        "/var/empty",
                    ]))
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(Command::new("/usr/bin/dscl").args([
                        ".",
                        "-create",
                        &format!("/Users/{name}"),
                        "UserShell",
                        "/sbin/nologin",
                    ]))
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(
                        Command::new("/usr/bin/dscl")
                            .args([
                                ".",
                                "-append",
                                &format!("/Groups/{groupname}"),
                                "GroupMembership",
                            ])
                            .arg(&name),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(Command::new("/usr/bin/dscl").args([
                        ".",
                        "-create",
                        &format!("/Users/{name}"),
                        "IsHidden",
                        "1",
                    ]))
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(
                        Command::new("/usr/sbin/dseditgroup")
                            .args(["-o", "edit"])
                            .arg("-a")
                            .arg(&name)
                            .arg("-t")
                            .arg(&name)
                            .arg(groupname),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                }
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
                .map_err(|e| CreateUserError::Command(e).boxed())?;
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
                groupname: _,
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
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            name,
            uid: _,
            groupname: _,
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
                // TODO(@hoverbear): Make this actually work...
                // Right now, our test machines do not have a secure token and cannot delete users.
                tracing::warn!("Harmonic currently cannot delete groups on Mac due to https://github.com/DeterminateSystems/harmonic/issues/33. This is a no-op, installing with harmonic again will use the existing user.");
                // execute_command(Command::new("/usr/bin/dscl").args([
                //     ".",
                //     "-delete",
                //     &format!("/Users/{name}"),
                // ]))
                // .await
                // .map_err(|e| CreateUserError::Command(e).boxed())?;
            },
            _ => {
                execute_command(Command::new("userdel").args([&name.to_string()]))
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
            },
        };

        tracing::trace!("Deleted user");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }

    fn action_state(&self) -> ActionState {
        self.action_state
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateUserError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
