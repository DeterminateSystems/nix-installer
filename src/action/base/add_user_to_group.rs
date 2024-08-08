use std::process::Stdio;

use nix::unistd::User;
use target_lexicon::OperatingSystem;
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind};
use crate::execute_command;

use crate::action::{Action, ActionDescription, StatefulAction};

/**
Create an operating system level user in the given group
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "add_user_to_group")]
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

        match OperatingSystem::host() {
            OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => (),
            _ => {
                if !(which::which("addgroup").is_ok() || which::which("gpasswd").is_ok()) {
                    return Err(Self::error(ActionErrorKind::MissingAddUserToGroupCommand));
                }
                if !(which::which("delgroup").is_ok() || which::which("gpasswd").is_ok()) {
                    return Err(Self::error(
                        ActionErrorKind::MissingRemoveUserFromGroupCommand,
                    ));
                }
            },
        }

        // Ensure user does not exists
        if let Some(user) = User::from_name(name.as_str())
            .map_err(|e| ActionErrorKind::GettingUserId(name.clone(), e))
            .map_err(Self::error)?
        {
            if user.uid.as_raw() != uid {
                return Err(Self::error(ActionErrorKind::UserUidMismatch(
                    name.clone(),
                    user.uid.as_raw(),
                    uid,
                )));
            }

            if user.gid.as_raw() != gid {
                return Err(Self::error(ActionErrorKind::UserGidMismatch(
                    name.clone(),
                    user.gid.as_raw(),
                    gid,
                )));
            }

            // See if group membership needs to be done
            match OperatingSystem::host() {
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
                    tracing::trace!("Executing `{:?}`", command.as_std());
                    let output = command
                        .output()
                        .await
                        .map_err(|e| ActionErrorKind::command(&command, e))
                        .map_err(Self::error)?;
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
                        },
                        _ => {
                            // Some other issue
                            return Err(Self::error(ActionErrorKind::command_output(
                                &command, output,
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
                    .map_err(Self::error)?;
                    let output_str = String::from_utf8(output.stdout).map_err(Self::error)?;
                    let user_in_group = output_str.split(' ').any(|v| v == this.groupname);

                    if user_in_group {
                        tracing::debug!(
                            "Adding user `{}` to group `{}` already complete",
                            this.name,
                            this.groupname
                        );
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
    fn action_tag() -> crate::action::ActionTag {
        crate::action::ActionTag("add_user_to_group")
    }
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
                .map_err(Self::error)?;
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
                .map_err(Self::error)?;
            },
            _ => {
                if which::which("gpasswd").is_ok() {
                    execute_command(
                        Command::new("gpasswd")
                            .process_group(0)
                            .args(["-a"])
                            .args([name, groupname])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                } else if which::which("addgroup").is_ok() {
                    execute_command(
                        Command::new("addgroup")
                            .process_group(0)
                            .args([name, groupname])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                } else {
                    return Err(Self::error(Self::error(
                        ActionErrorKind::MissingAddUserToGroupCommand,
                    )));
                }
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
                .map_err(Self::error)?;
            },
            _ => {
                if which::which("gpasswd").is_ok() {
                    execute_command(
                        Command::new("gpasswd")
                            .process_group(0)
                            .args(["-d"])
                            .args([&name.to_string(), &groupname.to_string()])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                } else if which::which("delgroup").is_ok() {
                    execute_command(
                        Command::new("delgroup")
                            .process_group(0)
                            .args([name, groupname])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                } else {
                    return Err(Self::error(
                        ActionErrorKind::MissingRemoveUserFromGroupCommand,
                    ));
                }
            },
        };

        Ok(())
    }
}
