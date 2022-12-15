use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};

/**
Kickstart a `launchctl` service
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct KickstartLaunchctlService {
    unit: String,
}

impl KickstartLaunchctlService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(unit: String) -> Result<StatefulAction<Self>, ActionError> {
        Ok(Self { unit }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kickstart_launchctl_service")]
impl Action for KickstartLaunchctlService {
    fn tracing_synopsis(&self) -> String {
        let Self { unit, .. } = self;
        format!("Kickstart the launchctl unit `{unit}`")
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "kickstart_launchctl_service",
            unit = %self.unit,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { unit } = self;

        execute_command(
            Command::new("launchctl")
                .process_group(0)
                .arg("kickstart")
                .arg("-k")
                .arg(unit)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(ActionError::Command)?;

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
