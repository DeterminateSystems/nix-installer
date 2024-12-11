use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};

/**
Set a time machine exclusion on a path.

Note, this cannot be used on Volumes easily:

```bash,no_run
% sudo tmutil addexclusion -v "Nix Store"
tmutil: addexclusion requires Full Disk Access privileges.
To allow this operation, select Full Disk Access in the Privacy
tab of the Security & Privacy preference pane, and add Terminal
to the list of applications which are allowed Full Disk Access.
% sudo tmutil addexclusion /nix
/nix: The operation couldn’t be completed. Invalid argument
```

 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "set_tmutil_exclusion")]
pub struct SetTmutilExclusion {
    path: PathBuf,
}

impl SetTmutilExclusion {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(path: impl AsRef<Path>) -> Result<StatefulAction<Self>, ActionError> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "set_tmutil_exclusion")]
impl Action for SetTmutilExclusion {
    fn action_tag() -> ActionTag {
        ActionTag("set_tmutil_exclusion")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Configure Time Machine exclusion on `{}`",
            self.path.display()
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "set_tmutil_exclusion",
            path = %self.path.display(),
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        execute_command(
            Command::new("tmutil")
                .process_group(0)
                .arg("addexclusion")
                .arg(&self.path)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        execute_command(
            Command::new("tmutil")
                .process_group(0)
                .arg("removeexclusion")
                .arg(&self.path)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }
}
