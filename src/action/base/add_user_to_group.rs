use std::process::Stdio;

use nix::unistd::User;
use target_lexicon::OperatingSystem;
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::ActionError;
use crate::execute_command;

use crate::action::{Action, ActionDescription, StatefulAction};

/**
Create an operating system level user in the given group
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct AddUserToGroup {
    name: String,
    uid: u32,
    groupname: String,
    gid: u32,
}

impl AddUserToGroup {
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

            // See if group membership needs to be done
            match target_lexicon::OperatingSystem::host() {
                OperatingSystem::MacOSX {
                    major: _,
                    minor: _,
                    patch: _,
                }
                | OperatingSystem::Darwin => {
                    let mut command = Command::new("/usr/sbin/dseditgroup");
                    command.process_group(0);
                    command.args(["-o", "checkmember", "-m"]);
                    command.arg(&this.name);
                    command.arg(&this.groupname);
                    command.stdout(Stdio::piped());
                    command.stderr(Stdio::piped());
                    let command_str = format!("{:?}", command.as_std());
                    tracing::trace!("Executing `{command_str}`");
                    let output = command.output().await.map_err(ActionError::Command)?;
                    match output.status.code() {
                        Some(0) => {
                            // yes {user} is a member of {groupname}
                            // Since the user exists, and is already a member of the group, we have truly nothing to do here
                            tracing::debug!(
                                "Adding user `{}` to group `{}` already complete",
                                this.name,
                                this.groupname
                            );
                            return Ok(StatefulAction::completed(this));
                        },
                        Some(64) => {
                            // 64 is the exit code for "Group not found"
                            tracing::trace!(
                                "Will add user `{}` to newly created group `{}`",
                                this.name,
                                this.groupname
                            );
                            // The group will be created by the installer
                            ()
                        },
                        _ => {
                            // Some other issue
                            return Err(ActionError::Command(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!(
                                    "Command `{command_str}` failed{}, stderr:\n{}\n",
                                    if let Some(code) = output.status.code() {
                                        format!(" status {code}")
                                    } else {
                                        "".to_string()
                                    },
                                    String::from_utf8_lossy(&output.stderr),
                                ),
                            )));
                        },
                    };
                },
                _ => {
                    let output = execute_command(
                        Command::new("groups")
                            .process_group(0)
                            .arg(&this.name)
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| ActionError::Command(e))?;
                    let output_str = String::from_utf8(output.stdout)?;
                    let user_in_group = output_str.split(" ").any(|v| v == &this.groupname);

                    if user_in_group {
                        tracing::debug!("Creating user `{}` already complete", this.name);
                        return Ok(StatefulAction::completed(this));
                    }
                },
            }
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "add_user_to_group")]
impl Action for AddUserToGroup {
    fn tracing_synopsis(&self) -> String {
        format!(
            "Add user `{}` (UID {}) to group `{}` (GID {})",
            self.name, self.uid, self.groupname, self.gid
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "add_user_to_group",
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
                "The Nix daemon requires the build users to be in a defined group"
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            name,
            uid: _,
            groupname,
            gid: _,
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
                .map_err(|e| ActionError::Command(e))?;
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
                .map_err(|e| ActionError::Command(e))?;
            },
            _ => {
                execute_command(
                    Command::new("gpasswd")
                        .process_group(0)
                        .args(["-a"])
                        .args([&name.to_string(), &groupname.to_string()])
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
                "Remove user `{}` (UID {}) from group {} (GID {})",
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
            groupname,
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
                execute_command(
                    Command::new("/usr/bin/dscl")
                        .process_group(0)
                        .args([".", "-delete", &format!("/Groups/{groupname}"), "users"])
                        .arg(&name)
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ActionError::Command(e))?;
            },
            _ => {
                execute_command(
                    Command::new("gpasswd")
                        .process_group(0)
                        .args(["-d"])
                        .args([&name.to_string(), &groupname.to_string()])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ActionError::Command(e))?;
            },
        };

        Ok(())
    }
}
