use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};
use crate::os::darwin::DiskUtilInfoOutput;

/**
Enable ownership on a volume
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "enable_ownership")]
pub struct EnableOwnership {
    path: PathBuf,
}

impl EnableOwnership {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(path: impl AsRef<Path>) -> Result<StatefulAction<Self>, ActionError> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "enable_ownership")]
impl Action for EnableOwnership {
    fn action_tag() -> ActionTag {
        ActionTag("enable_ownership")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Enable ownership on `{}`", self.path.display())
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "enable_ownership",
            path = %self.path.display(),
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let should_enable_ownership = {
            let the_plist = DiskUtilInfoOutput::for_volume_path(&self.path)
                .await
                .map_err(Self::error)?;

            !the_plist.global_permissions_enabled
        };

        if should_enable_ownership {
            execute_command(
                Command::new("/usr/sbin/diskutil")
                    .process_group(0)
                    .arg("enableOwnership")
                    .arg(&self.path)
                    .stdin(std::process::Stdio::null()),
            )
            .await
            .map_err(Self::error)?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        // noop
        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum EnableOwnershipError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}

impl From<EnableOwnershipError> for ActionErrorKind {
    fn from(val: EnableOwnershipError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}
