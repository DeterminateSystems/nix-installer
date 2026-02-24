use tracing::{span, Span};

use crate::action::base::RemoveDirectory;
use crate::action::{Action, ActionDescription};
use crate::action::{ActionError, StatefulAction};

/** Cleanup after a successful installation. Does nothing on revert.
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "cleanup")]
pub struct Cleanup {
    remove_scratch_dir: StatefulAction<RemoveDirectory>,
}

impl Cleanup {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let remove_scratch_dir = RemoveDirectory::plan(crate::settings::SCRATCH_DIR)
            .await
            .map_err(Self::error)?;

        Ok(StatefulAction::uncompleted(Self { remove_scratch_dir }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "cleanup")]
impl Action for Cleanup {
    fn action_tag() -> crate::action::ActionTag {
        crate::action::ActionTag("cleanup")
    }
    fn tracing_synopsis(&self) -> String {
        String::from("Cleanup")
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "cleanup",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let explanation = vec![self.remove_scratch_dir.tracing_synopsis()];
        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        self.remove_scratch_dir.try_execute().await?;

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
