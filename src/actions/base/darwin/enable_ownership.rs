use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::Command;

use crate::execute_command;

use crate::actions::{ActionDescription, ActionError, ActionState, Actionable};
use crate::os::darwin::DiskUtilOutput;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct EnableOwnership {
    path: PathBuf,
    action_state: ActionState,
}

impl EnableOwnership {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "enable-ownership")]
impl Actionable for EnableOwnership {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Enable ownership on {}", self.path.display()),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { path, action_state } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Enabling ownership");
            return Ok(());
        }
        tracing::debug!("Enabling ownership");

        let should_enable_ownership = {
            let buf = execute_command(
                Command::new("/usr/sbin/diskutil")
                    .args(["info", "-plist"])
                    .arg(&path),
            )
            .await
            .unwrap()
            .stdout;
            let the_plist: DiskUtilOutput = plist::from_reader(Cursor::new(buf)).unwrap();

            the_plist.global_permissions_enabled
        };

        if should_enable_ownership {
            execute_command(
                Command::new("/usr/sbin/diskutil")
                    .arg("enableOwnership")
                    .arg(path),
            )
            .await
            .map_err(|e| EnableOwnershipError::Command(e).boxed())?;
        }

        tracing::trace!("Enabled ownership");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![]
        }
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            path: _,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Unenabling ownership (noop)");
            return Ok(());
        }
        tracing::debug!("Unenabling ownership (noop)");

        tracing::trace!("Unenabled ownership (noop)");
        *action_state = ActionState::Completed;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum EnableOwnershipError {
    #[error("Failed to execute command")]
    Command(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
