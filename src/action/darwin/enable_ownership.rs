use std::io::Cursor;
use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::execute_command;

use crate::os::darwin::DiskUtilOutput;
use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

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
#[typetag::serde(name = "enable_ownership")]
impl Action for EnableOwnership {
    fn tracing_synopsis(&self) -> String {
        format!("Enable ownership on {}", self.path.display())
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            path,
            action_state: _,
        } = self;

        let should_enable_ownership = {
            let buf = execute_command(
                Command::new("/usr/sbin/diskutil")
                    .args(["info", "-plist"])
                    .arg(&path)
                    .stdin(std::process::Stdio::null()),
            )
            .await?
            .stdout;
            let the_plist: DiskUtilOutput = plist::from_reader(Cursor::new(buf)).unwrap();

            the_plist.global_permissions_enabled
        };

        if should_enable_ownership {
            execute_command(
                Command::new("/usr/sbin/diskutil")
                    .arg("enableOwnership")
                    .arg(path)
                    .stdin(std::process::Stdio::null()),
            )
            .await
            .map_err(|e| EnableOwnershipError::Command(e).boxed())?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![]
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // noop
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
pub enum EnableOwnershipError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
