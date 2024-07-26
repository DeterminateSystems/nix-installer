use nix::unistd::Group;
use target_lexicon::OperatingSystem;
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag};
use crate::execute_command;

use crate::action::{Action, ActionDescription, StatefulAction};

/**
Create an operating system level user group
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_group")]
pub struct CreateGroup {
    name: String,
    gid: u32,
}

impl CreateGroup {
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn plan(name: String, gid: u32) -> Result<StatefulAction<Self>, ActionError> {
        let this = Self {
            name: name.clone(),
            gid,
        };

        match OperatingSystem::host() {
            OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => (),
            _ => {
                if !(which::which("groupadd").is_ok() || which::which("addgroup").is_ok()) {
                    return Err(Self::error(ActionErrorKind::MissingGroupCreationCommand));
                }
                if !(which::which("groupdel").is_ok() || which::which("delgroup").is_ok()) {
                    return Err(Self::error(ActionErrorKind::MissingGroupDeletionCommand));
                }
            },
        }

        // Ensure group does not exists
        if let Some(group) = Group::from_name(name.as_str())
            .map_err(|e| ActionErrorKind::GettingGroupId(name.clone(), e))
            .map_err(Self::error)?
        {
            if group.gid.as_raw() != gid {
                return Err(Self::error(ActionErrorKind::GroupGidMismatch(
                    name.clone(),
                    group.gid.as_raw(),
                    gid,
                )));
            }

            tracing::debug!("Creating group `{}` already complete", this.name);
            return Ok(StatefulAction::completed(this));
        }
        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_group")]
impl Action for CreateGroup {
    fn action_tag() -> ActionTag {
        ActionTag("create_group")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Create group `{}` (GID {})", self.name, self.gid)
    }
    fn execute_description(&self) -> Vec<ActionDescription> {
        let Self { name: _, gid: _ } = &self;
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!(
                "The nix daemon requires a system user group its system users can be part of"
            )],
        )]
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "create_group",
            user = self.name,
            gid = self.gid,
        )
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { name, gid } = self;

        use OperatingSystem;
        match OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                execute_command(
                    Command::new("/usr/sbin/dseditgroup")
                        .process_group(0)
                        .args([
                            "-o",
                            "create",
                            "-r",
                            "Nix build group for nix-daemon",
                            "-i",
                            &format!("{gid}"),
                            name,
                        ])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(Self::error)?;
            },
            _ => {
                if which::which("groupadd").is_ok() {
                    execute_command(
                        Command::new("groupadd")
                            .process_group(0)
                            .args(["-g", &gid.to_string(), "--system", name])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                } else if which::which("addgroup").is_ok() {
                    execute_command(
                        Command::new("addgroup")
                            .process_group(0)
                            .args(["-g", &gid.to_string(), "--system", name])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                } else {
                    return Err(Self::error(ActionErrorKind::MissingGroupCreationCommand));
                }
            },
        };

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self { name, gid } = &self;
        vec![ActionDescription::new(
            format!("Delete group `{name}` (GID {gid})"),
            vec![format!(
                "The nix daemon requires a system user group its system users can be part of"
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self { name, gid: _ } = self;

        use OperatingSystem;
        match OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                execute_command(
                    Command::new("/usr/bin/dscl")
                        .args([".", "-delete", &format!("/Groups/{name}")])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(Self::error)?;
            },
            _ => {
                if which::which("groupdel").is_ok() {
                    execute_command(
                        Command::new("groupdel")
                            .process_group(0)
                            .arg(name)
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                } else if which::which("delgroup").is_ok() {
                    execute_command(
                        Command::new("delgroup")
                            .process_group(0)
                            .arg(name)
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                } else {
                    return Err(Self::error(ActionErrorKind::MissingGroupDeletionCommand));
                }
            },
        };

        Ok(())
    }
}
