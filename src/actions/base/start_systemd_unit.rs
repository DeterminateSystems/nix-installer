use serde::Serialize;
use tokio::process::Command;

use crate::actions::meta::StartNixDaemon;
use crate::{execute_command, HarmonicError};

use crate::actions::{ActionDescription, Actionable, ActionState, Action, ActionError};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartSystemdUnit {
    unit: String,
}

impl StartSystemdUnit {
    #[tracing::instrument(skip_all)]
    pub async fn plan(unit: String) -> Result<ActionState<Self>, StartSystemdUnitError> {
        Ok(ActionState::Planned(Self { unit }))
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<StartSystemdUnit> {
    type Error = StartSystemdUnitError;
    fn description(&self) -> Vec<ActionDescription> {
        match self {
            ActionState::Planned(v) => vec![
                ActionDescription::new(
                    "Start the systemd Nix service and socket".to_string(),
                    vec![
                        "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                    ]
                ),
            ],
            ActionState::Completed(_) => vec![
                ActionDescription::new(
                    "Stop the systemd Nix service and socket".to_string(),
                    vec![
                        "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                    ]
                ),
            ],
            ActionState::Reverted(_) => vec![
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
    async fn execute(&mut self) -> Result<(), ActionError> {
        let StartSystemdUnit { unit } = match self {
            ActionState::Completed(_) => return Err(ActionError::AlreadyExecuted(self.clone().into())),
            ActionState::Reverted(_) => return Err(ActionError::AlreadyReverted(self.clone().into())),
            ActionState::Planned(v) => v,
        };
        // TODO(@Hoverbear): Handle proxy vars
        execute_command(
            Command::new("systemctl")
                .arg("enable")
                .arg("--now")
                .arg(format!("{unit}")),
        )
        .await.map_err(StartSystemdUnitError::Command)?;

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<StartSystemdUnit>> for ActionState<Action> {
    fn from(v: ActionState<StartSystemdUnit>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::StartSystemdUnit(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::StartSystemdUnit(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::StartSystemdUnit(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum StartSystemdUnitError {
    #[error("Failed to execute command")]
    #[serde(serialize_with = "crate::serialize_std_io_error_to_display")]
    Command(#[source] std::io::Error)
}
