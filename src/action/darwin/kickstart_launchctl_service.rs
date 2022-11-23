use tokio::process::Command;

use crate::execute_command;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct KickstartLaunchctlService {
    unit: String,
    action_state: ActionState,
}

impl KickstartLaunchctlService {
    #[tracing::instrument(skip_all)]
    pub async fn plan(unit: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            unit,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kickstart_launchctl_service")]
impl Action for KickstartLaunchctlService {
    fn tracing_synopsis(&self) -> String {
        let Self { unit, .. } = self;
        format!("Kickstart the launchctl unit `{unit}`")
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        unit = %self.unit,
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            unit,
            action_state: _,
        } = self;

        execute_command(
            Command::new("launchctl")
                .process_group(0)
                .arg("kickstart")
                .arg("-k")
                .arg(unit)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| KickstartLaunchctlServiceError::Command(e).boxed())?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![]
    }

    #[tracing::instrument(skip_all, fields(
        unit = %self.unit,
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
pub enum KickstartLaunchctlServiceError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
