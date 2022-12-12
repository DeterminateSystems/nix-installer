use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::action::{ActionError, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};

/**
Bootstrap and kickstart an APFS volume
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct BootstrapApfsVolume {
    path: PathBuf,
}

impl BootstrapApfsVolume {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(path: impl AsRef<Path>) -> Result<StatefulAction<Self>, ActionError> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "bootstrap_volume")]
impl Action for BootstrapApfsVolume {
    fn tracing_synopsis(&self) -> String {
        format!("Bootstrap and kickstart `{}`", self.path.display())
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { path } = self;

        execute_command(
            Command::new("launchctl")
                .process_group(0)
                .args(["bootstrap", "system"])
                .arg(path)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| ActionError::Command(e))?;
        execute_command(
            Command::new("launchctl")
                .process_group(0)
                .args(["kickstart", "-k", "system/org.nixos.darwin-store"])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| ActionError::Command(e))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Stop `{}`", self.path.display()),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        path = %self.path.display(),
    ))]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self { path } = self;

        execute_command(
            Command::new("launchctl")
                .process_group(0)
                .args(["bootout", "system"])
                .arg(path)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| ActionError::Command(e))?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BootstrapVolumeError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
