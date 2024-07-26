use std::path::{Path, PathBuf};

use tokio::fs::remove_dir_all;
use tracing::{span, Span};

use crate::action::{Action, ActionDescription, ActionErrorKind, ActionState};
use crate::action::{ActionError, StatefulAction};

/** Remove a directory, does nothing on revert.
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "remove_directory")]
pub struct RemoveDirectory {
    path: PathBuf,
}

impl RemoveDirectory {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(path: impl AsRef<Path>) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref().to_path_buf();

        Ok(StatefulAction {
            action: Self { path },
            state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "remove_directory")]
impl Action for RemoveDirectory {
    fn action_tag() -> crate::action::ActionTag {
        crate::action::ActionTag("remove_directory")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Remove directory `{}`", self.path.display())
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "remove_directory",
            path = tracing::field::display(self.path.display()),
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        if self.path.exists() {
            if !self.path.is_dir() {
                return Err(Self::error(ActionErrorKind::PathWasNotDirectory(
                    self.path.clone(),
                )));
            }
            remove_dir_all(&self.path)
                .await
                .map_err(|e| Self::error(ActionErrorKind::Remove(self.path.clone(), e)))?;
        } else {
            tracing::debug!("Directory `{}` not present, skipping", self.path.display(),);
        };

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        Ok(())
    }
}
