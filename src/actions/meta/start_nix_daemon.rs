use serde::Serialize;

use crate::actions::base::{StartSystemdUnit, StartSystemdUnitError};

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

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
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for StartNixDaemon {
    type Error = StartNixDaemonError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            self.start_systemd_socket.describe_execute()
        }
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            start_systemd_socket,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Starting the nix daemon");
            return Ok(());
        }
        *action_state = ActionState::Progress;
        tracing::debug!("Starting the nix daemon");

        start_systemd_socket.execute().await?;

        tracing::trace!("Started the nix daemon");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            self.start_systemd_socket.describe_revert()
        }
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            start_systemd_socket,
            action_state,
            ..
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Stop the nix daemon");
            return Ok(());
        }
        *action_state = ActionState::Progress;
        tracing::debug!("Stop the nix daemon");

        start_systemd_socket.revert().await?;

        tracing::trace!("Stopped the nix daemon");
        *action_state = ActionState::Uncompleted;
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
    #[error("Starting systemd unit")]
    StartSystemdUnit(
        #[source]
        #[from]
        StartSystemdUnitError,
    ),
}
