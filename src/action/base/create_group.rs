use nix::unistd::Group;
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionTag};
use crate::execute_command;

use crate::action::{Action, ActionDescription, StatefulAction};

/**
Create an operating system level user group
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
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
        // Ensure group does not exists
        if let Some(group) = Group::from_name(name.as_str())
            .map_err(|e| ActionError::GettingGroupId(name.clone(), e))?
        {
            if group.gid.as_raw() != gid {
                return Err(ActionError::GroupGidMismatch(
                    name.clone(),
                    group.gid.as_raw(),
                    gid,
                ));
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

        use target_lexicon::OperatingSystem;
        match target_lexicon::OperatingSystem::host() {
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
                            name.as_str(),
                        ])
                        .stdin(std::process::Stdio::null()),
                )
                .await?;
            },
            _ => {
                execute_command(
                    Command::new("groupadd")
                        .process_group(0)
                        .args(["-g", &gid.to_string(), "--system", &name])
                        .stdin(std::process::Stdio::null()),
                )
                .await?;
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

        use target_lexicon::OperatingSystem;
        match target_lexicon::OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                let output = execute_command(
                    Command::new("/usr/bin/dscl")
                        .args([".", "-delete", &format!("/Groups/{name}")])
                        .stdin(std::process::Stdio::null()),
                )
                .await?;
                if !output.status.success() {}
            },
            _ => {
                execute_command(
                    Command::new("groupdel")
                        .process_group(0)
                        .arg(&name)
                        .stdin(std::process::Stdio::null()),
                )
                .await?;
            },
        };

        Ok(())
    }
}
