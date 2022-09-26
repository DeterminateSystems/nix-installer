use serde::Serialize;
use tokio::process::Command;

use crate::execute_command;

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartSystemdUnit {
    unit: String,
    action_state: ActionState,
}

impl StartSystemdUnit {
    #[tracing::instrument(skip_all)]
    pub async fn plan(unit: String) -> Result<Self, StartSystemdUnitError> {
        Ok(Self {
            unit,
            action_state: ActionState::Planned,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for StartSystemdUnit {
    type Error = StartSystemdUnitError;
    fn description(&self) -> Vec<ActionDescription> {
        match self.action_state {
            ActionState::Planned => vec![
                ActionDescription::new(
                    "Start the systemd Nix service and socket".to_string(),
                    vec![
                        "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                    ]
                ),
            ],
            ActionState::Completed => vec![
                ActionDescription::new(
                    "Stop the systemd Nix service and socket".to_string(),
                    vec![
                        "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                    ]
                ),
            ],
            ActionState::Reverted => vec![
                ActionDescription::new(
                    "Stopped the systemd Nix service and socket".to_string(),
                    vec![
                        "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                    ]
                ),
            ],
        }
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self { unit, action_state } = self;

        // TODO(@Hoverbear): Handle proxy vars
        execute_command(
            Command::new("systemctl")
                .arg("enable")
                .arg("--now")
                .arg(format!("{unit}")),
        )
        .await
        .map_err(StartSystemdUnitError::Command)?;

        *action_state = ActionState::Completed;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self { unit, action_state } = self;

        // TODO(@Hoverbear): Handle proxy vars
        execute_command(Command::new("systemctl").arg("stop").arg(format!("{unit}")))
            .await
            .map_err(StartSystemdUnitError::Command)?;

        *action_state = ActionState::Reverted;
        Ok(())
    }
}

impl From<StartSystemdUnit> for Action {
    fn from(v: StartSystemdUnit) -> Self {
        Action::StartSystemdUnit(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum StartSystemdUnitError {
    #[error("Failed to execute command")]
    Command(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
