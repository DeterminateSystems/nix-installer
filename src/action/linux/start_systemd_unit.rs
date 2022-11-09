use tokio::process::Command;

use crate::execute_command;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartSystemdUnit {
    unit: String,
    action_state: ActionState,
}

impl StartSystemdUnit {
    #[tracing::instrument(skip_all)]
    pub async fn plan(unit: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            unit,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "start_systemd_unit")]
impl Action for StartSystemdUnit {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Start the systemd Nix service and socket".to_string(),
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
            tracing::trace!("Already completed: Starting systemd unit");
            return Ok(());
        }
        tracing::debug!("Starting systemd unit");

        // TODO(@Hoverbear): Handle proxy vars
        execute_command(
            Command::new("systemctl")
                .arg("enable")
                .arg("--now")
                .arg(format!("{unit}")),
        )
        .await
        .map_err(|e| StartSystemdUnitError::Command(e).boxed())?;

        tracing::trace!("Started systemd unit");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Stop the systemd Nix service and socket".to_string(),
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
        let Self { unit, action_state } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Stopping systemd unit");
            return Ok(());
        }
        tracing::debug!("Stopping systemd unit");

        // TODO(@Hoverbear): Handle proxy vars
        execute_command(
            Command::new("systemctl")
                .arg("disable")
                .arg(format!("{unit}")),
        )
        .await
        .map_err(|e| StartSystemdUnitError::Command(e).boxed())?;

        tracing::trace!("Stopped systemd unit");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn action_state(&self) -> ActionState {
        self.action_state
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartSystemdUnitError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
