use crate::actions::base::{StartSystemdUnit, StartSystemdUnitReceipt};
use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, Revertable};

/// This is mostly indirection for supporting non-systemd
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemon {
    start_systemd_socket: StartSystemdUnit,
}

impl StartNixDaemon {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<Self, HarmonicError> {
        let start_systemd_socket = StartSystemdUnit::plan("nix-daemon.socket".into()).await?;
        let start_systemd_service = StartSystemdUnit::plan("nix-daemon.service".into()).await?;
        Ok(Self {
            start_systemd_socket,
        })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for StartNixDaemon {
    type Receipt = StartNixDaemonReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            start_systemd_socket,
        } = &self;
        start_systemd_socket.description()
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self {
            start_systemd_socket,
        } = self;
        let start_systemd_socket = start_systemd_socket.execute().await?;
        Ok(Self::Receipt {
            start_systemd_socket,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemonReceipt {
    start_systemd_socket: StartSystemdUnitReceipt,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for StartNixDaemonReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
