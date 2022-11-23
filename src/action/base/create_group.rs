use tokio::process::Command;

use crate::execute_command;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateGroup {
    name: String,
    gid: usize,
    action_state: ActionState,
}

impl CreateGroup {
    #[tracing::instrument(skip_all)]
    pub fn plan(name: String, gid: usize) -> Self {
        Self {
            name,
            gid,
            action_state: ActionState::Uncompleted,
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_group")]
impl Action for CreateGroup {
    fn tracing_synopsis(&self) -> String {
        format!("Create group `{}` with GID `{}`", self.name, self.gid)
    }
    fn execute_description(&self) -> Vec<ActionDescription> {
        let Self {
            name: _,
            gid: _,
            action_state: _,
        } = &self;
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!(
                "The nix daemon requires a system user group its system users can be part of"
            )],
        )]
    }

    #[tracing::instrument(skip_all, fields(
        user = self.name,
        gid = self.gid,
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            name,
            gid,
            action_state: _,
        } = self;

        let process_group =
            nix::unistd::setsid().map_err(|e| CreateGroupError::ProcessGroupCreation(e))?;

        use target_lexicon::OperatingSystem;
        match target_lexicon::OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                if Command::new("/usr/bin/dscl")
                    .process_group(process_group.as_raw())
                    .args([".", "-read", &format!("/Groups/{name}")])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .status()
                    .await?
                    .success()
                {
                    ()
                } else {
                    execute_command(
                        Command::new("/usr/sbin/dseditgroup")
                            .process_group(process_group.as_raw())
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
                    .await
                    .map_err(|e| CreateGroupError::Command(e).boxed())?;
                }
            },
            _ => {
                execute_command(
                    Command::new("groupadd")
                        .process_group(process_group.as_raw())
                        .args(["-g", &gid.to_string(), "--system", &name])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| CreateGroupError::Command(e).boxed())?;
            },
        };

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            name,
            gid: _,
            action_state: _,
        } = &self;
        vec![ActionDescription::new(
            format!("Delete group {name}"),
            vec![format!(
                "The nix daemon requires a system user group its system users can be part of"
            )],
        )]
    }

    #[tracing::instrument(skip_all, fields(
        user = self.name,
        gid = self.gid,
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            name,
            gid: _,
            action_state: _,
        } = self;

        let process_group =
            nix::unistd::setsid().map_err(|e| CreateGroupError::ProcessGroupCreation(e))?;

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
                tracing::warn!("Harmonic currently cannot delete groups on Mac due to https://github.com/DeterminateSystems/harmonic/issues/33. This is a no-op, installing with harmonic again will use the existing group.");
                // execute_command(Command::new("/usr/bin/dscl").args([
                //     ".",
                //     "-delete",
                //     &format!("/Groups/{name}"),
                // ]).stdin(std::process::Stdio::null()))
                // .await
                // .map_err(|e| CreateGroupError::Command(e).boxed())?;
            },
            _ => {
                execute_command(
                    Command::new("groupdel")
                        .process_group(process_group.as_raw())
                        .arg(&name)
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| CreateGroupError::Command(e).boxed())?;
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
pub enum CreateGroupError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
    #[error("Could not create process grouip via `setsid`")]
    ProcessGroupCreation(#[source] nix::Error),
}
