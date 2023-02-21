use nix::unistd::User;
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::ActionError;
use crate::execute_command;

use crate::action::{Action, ActionDescription, StatefulAction};

/**
Create an operating system level user in the given group
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUser {
    name: String,
    uid: u32,
    groupname: String,
    gid: u32,
}

impl CreateUser {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        name: String,
        uid: u32,
        groupname: String,
        gid: u32,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let this = Self {
            name: name.clone(),
            uid,
            groupname,
            gid,
        };
        // Ensure user does not exists
        if let Some(user) = User::from_name(name.as_str())
            .map_err(|e| ActionError::GettingUserId(name.clone(), e))?
        {
            if user.uid.as_raw() != uid {
                return Err(ActionError::UserUidMismatch(
                    name.clone(),
                    user.uid.as_raw(),
                    uid,
                ));
            }

            if user.gid.as_raw() != gid {
                return Err(ActionError::UserGidMismatch(
                    name.clone(),
                    user.gid.as_raw(),
                    gid,
                ));
            }
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_user")]
impl Action for CreateUser {
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create user `{}` (UID {}) in group `{}` (GID {})",
            self.name, self.uid, self.groupname, self.gid
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "create_user",
            user = self.name,
            uid = self.uid,
            groupname = self.groupname,
            gid = self.gid,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!(
                "The Nix daemon requires system users it can act as in order to build"
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            name,
            uid,
            groupname: _,
            gid,
        } = self;

        use target_lexicon::OperatingSystem;
        match OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                execute_command(
                    Command::new("/usr/bin/dscl")
                        .process_group(0)
                        .args([".", "-create", &format!("/Users/{name}")])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ActionError::Command(e))?;
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
                .map_err(|e| ActionError::Command(e))?;
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
                .map_err(|e| ActionError::Command(e))?;
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
                .map_err(|e| ActionError::Command(e))?;
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
                .map_err(|e| ActionError::Command(e))?;
                execute_command(
                    Command::new("/usr/bin/dscl")
                        .process_group(0)
                        .args([".", "-create", &format!("/Users/{name}"), "IsHidden", "1"])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ActionError::Command(e))?;
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
                .map_err(|e| ActionError::Command(e))?;
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
                "The Nix daemon requires system users it can act as in order to build"
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            name,
            uid: _,
            groupname: _,
            gid: _,
        } = self;

        use target_lexicon::OperatingSystem;
        match target_lexicon::OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                // MacOS is a "Special" case
                // It's only possible to delete users under certain conditions.
                // Documentation on https://it.megocollector.com/macos/cant-delete-a-macos-user-with-dscl-resolution/ and http://www.aixperts.co.uk/?p=214 suggested it was a secure token
                // That is correct, however it's a bit more nuanced. It appears to be that a user must be graphically logged in for some other user on the system to be deleted.
                let mut command = Command::new("/usr/bin/dscl");
                command.args([".", "-delete", &format!("/Users/{name}")]);
                command.process_group(0);
                command.stdin(std::process::Stdio::null());

                let output = command
                    .output()
                    .await
                    .map_err(|e| ActionError::Command(e))?;
                let stderr = String::from_utf8(output.stderr)?;
                match output.status.code() {
                    Some(0) => (),
                    Some(40) if stderr.contains("-14120") => {
                        // The user is on an ephemeral Mac, like detsys uses
                        // These Macs cannot always delete users, as sometimes there is no graphical login
                        tracing::warn!("Encountered an exit code 40 with -14120 error while removing user, this is likely because the initial executing user did not have a secure token, or that there was no graphical login session. To delete the user, log in graphically, then in a shell (which may be over ssh) run `/usr/bin/dscl . -delete /Users/{name}");
                    },
                    status => {
                        let command_str = format!("{:?}", command.as_std());
                        // Something went wrong
                        return Err(ActionError::Command(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!(
                                "Command `{command_str}` failed{}, stderr:\n{}\n",
                                if let Some(status) = status {
                                    format!(" {status}")
                                } else {
                                    "".to_string()
                                },
                                stderr
                            ),
                        )));
                    },
                }
            },
            _ => {
                execute_command(
                    Command::new("userdel")
                        .process_group(0)
                        .args([&name.to_string()])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ActionError::Command(e))?;
            },
        };

        Ok(())
    }
}
