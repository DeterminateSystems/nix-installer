use serde::Serialize;

use crate::actions::base::{StartSystemdUnit, StartSystemdUnitError};
use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, ActionState, Action, ActionError};

/// This is mostly indirection for supporting non-systemd
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemon {
    start_systemd_socket: StartSystemdUnit,
    action_state: ActionState,
}

impl StartNixDaemon {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<Self, StartNixDaemonError> {
        let start_systemd_socket = StartSystemdUnit::plan("nix-daemon.socket".into()).await?;
        Ok(Self {
            start_systemd_socket,
            action_state: ActionState::Planned,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for StartNixDaemon {
    type Error = StartNixDaemonError;

    fn description(&self) -> Vec<ActionDescription> {
        self.start_systemd_socket.description()
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self { start_systemd_socket, action_state } = self;

        start_systemd_socket.execute().await?;

        *action_state = ActionState::Completed;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self { start_systemd_socket, action_state, .. } = self;

        start_systemd_socket.revert().await?;

        *action_state = ActionState::Reverted;
        Ok(())
    }
}

impl From<StartNixDaemon> for Action {
    fn from(v: StartNixDaemon) -> Self {
        Action::StartNixDaemon(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum StartNixDaemonError {
    #[error(transparent)]
    StartSystemdUnit(#[from] StartSystemdUnitError)
}
