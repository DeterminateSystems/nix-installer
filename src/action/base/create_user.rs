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
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create user `{}` (UID {}) in group {} (GID {})",
            self.name, self.uid, self.groupname, self.gid
        )
    }
    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!(
                "The nix daemon requires system users it can act as in order to build"
            )],
        )]
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
            action_state: _,
        } = self;

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
                    .process_group(0)
                    .args([".", "-read", &format!("/Users/{name}")])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .status()
                    .await?
                    .success()
                {
                    ()
                } else {
                    execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            .args([".", "-create", &format!("/Users/{name}")])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            .args([
                                ".",
                                "-create",
                                &format!("/Users/{name}"),
                                "UniqueID",
                                &format!("{uid}"),
                            ])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            .args([
                                ".",
                                "-create",
                                &format!("/Users/{name}"),
                                "PrimaryGroupID",
                                &format!("{gid}"),
                            ])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            .args([
                                ".",
                                "-create",
                                &format!("/Users/{name}"),
                                "NFSHomeDirectory",
                                "/var/empty",
                            ])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            .args([
                                ".",
                                "-create",
                                &format!("/Users/{name}"),
                                "UserShell",
                                "/sbin/nologin",
                            ])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            .args([
                                ".",
                                "-append",
                                &format!("/Groups/{groupname}"),
                                "GroupMembership",
                            ])
                            .arg(&name)
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(
                        Command::new("/usr/bin/dscl")
                            .process_group(0)
                            .args([".", "-create", &format!("/Users/{name}"), "IsHidden", "1"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                    execute_command(
                        Command::new("/usr/sbin/dseditgroup")
                            .process_group(0)
                            .args(["-o", "edit"])
                            .arg("-a")
                            .arg(&name)
                            .arg("-t")
                            .arg(&name)
                            .arg(groupname)
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| CreateUserError::Command(e).boxed())?;
                }
            },
            _ => {
                execute_command(
                    Command::new("useradd")
                        .process_group(0)
                        .args([
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
                        ])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| CreateUserError::Command(e).boxed())?;
            },
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!(
                "Delete user `{}` (UID {}) in group {} (GID {})",
                self.name, self.uid, self.groupname, self.gid
            ),
            vec![format!(
                "The nix daemon requires system users it can act as in order to build"
            )],
        )]
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
            action_state: _,
        } = self;

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
                // ]).stdin(std::process::Stdio::null()))
                // .await
                // .map_err(|e| CreateUserError::Command(e).boxed())?;
            },
            _ => {
                execute_command(
                    Command::new("userdel")
                        .process_group(0)
                        .args([&name.to_string()])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| CreateUserError::Command(e).boxed())?;
            },
        };

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
pub enum CreateUserError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
