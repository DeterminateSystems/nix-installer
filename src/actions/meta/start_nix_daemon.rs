use serde::Serialize;

use crate::actions::base::{StartSystemdUnit, StartSystemdUnitError};
use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, ActionState, Action, ActionError};

/// This is mostly indirection for supporting non-systemd
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemon {
    start_systemd_socket: ActionState<StartSystemdUnit>,
}

impl StartNixDaemon {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<ActionState<Self>, StartNixDaemonError> {
        let start_systemd_socket = StartSystemdUnit::plan("nix-daemon.socket".into()).await?;
        Ok(ActionState::Planned(Self {
            start_systemd_socket,
        }))
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<StartNixDaemon> {
    type Error = StartNixDaemonError;

    fn description(&self) -> Vec<ActionDescription> {
        let StartNixDaemon { start_systemd_socket } = match self {
            ActionState::Completed(v) | ActionState::Reverted(v) | ActionState::Planned(v) => v,
        };
        start_systemd_socket.description()
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<Self, ActionError> {
        let StartNixDaemon { start_systemd_socket } = match self {
            ActionState::Planned(v) => v,
            ActionState::Completed(_) => return Err(ActionError::AlreadyExecuted(self.clone().into())),
            ActionState::Reverted(_) => return Err(ActionError::AlreadyReverted(self.clone().into())),
        };

        start_systemd_socket.execute().await?;

        Ok(Self::Completed(StartNixDaemon {
            start_systemd_socket,
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<Self, ActionError> {
        let StartNixDaemon { start_systemd_socket } = match self {
            ActionState::Planned(v) => return Err(ActionError::NotExecuted(self.clone().into())),
            ActionState::Completed(v) => v,
            ActionState::Reverted(_) => return Err(ActionError::AlreadyReverted(self.clone().into())),
        };

        start_systemd_socket.revert().await?;

        Ok(Self::Reverted(StartNixDaemon {
            start_systemd_socket,
        }))
    }
}

impl From<ActionState<StartNixDaemon>> for ActionState<Action> {
    fn from(v: ActionState<StartNixDaemon>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::StartNixDaemon(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::StartNixDaemon(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::StartNixDaemon(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum StartNixDaemonError {
    #[error(transparent)]
    StartSystemdUnit(#[from] StartSystemdUnitError)
}
