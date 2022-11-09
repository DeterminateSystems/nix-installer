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
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self { unit, action_state } = self;
        if *action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Kickstart the launchctl unit `{unit}`"),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        unit = %self.unit,
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { unit, action_state } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Kickstarting launchctl unit");
            return Ok(());
        }
        tracing::debug!("Kickstarting launchctl unit");

        execute_command(
            Command::new("launchctl")
                .arg("kickstart")
                .arg("-k")
                .arg(unit)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| KickstartLaunchctlServiceError::Command(e).boxed())?;

        tracing::trace!("Kickstarted launchctl unit");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Kick".to_string(),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        unit = %self.unit,
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            unit: _,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Unkickstart launchctl unit (noop)");
            return Ok(());
        }
        tracing::debug!("Unkickstart launchctl unit (noop)");

        tracing::trace!("Unkickstart launchctl unit (noop)");
        *action_state = ActionState::Completed;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KickstartLaunchctlServiceError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
