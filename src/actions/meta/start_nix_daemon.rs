use crate::actions::base::{ConfigureNixDaemonServiceReceipt, ConfigureNixDaemonService, StartSystemdUnit, StartSystemdUnitReceipt};
use crate::{HarmonicError, InstallSettings};

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

/// This is mostly indirection for supporting non-systemd
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemon {
    start_systemd_socket: StartSystemdUnit,    
    start_systemd_service: StartSystemdUnit,
}

impl StartNixDaemon {
    pub async fn plan(settings: InstallSettings) -> Result<Self, HarmonicError> {
        let start_systemd_socket = StartSystemdUnit::plan("nix-daemon.socket".into()).await?;
        let start_systemd_service = StartSystemdUnit::plan("nix-daemon.service".into()).await?;
        Ok(Self { start_systemd_socket, start_systemd_service })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for StartNixDaemon {
    type Receipt = StartNixDaemonReceipt;
    fn description(&self) -> Vec<ActionDescription> {
       let Self { start_systemd_socket, start_systemd_service } = &self;
       start_systemd_service.description()
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { start_systemd_socket, start_systemd_service } = self;
        let start_systemd_service = start_systemd_service.execute().await?;
        let start_systemd_socket = start_systemd_socket.execute().await?;
        Ok(Self::Receipt { start_systemd_socket, start_systemd_service })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemonReceipt {
    start_systemd_socket: StartSystemdUnitReceipt,
    start_systemd_service: StartSystemdUnitReceipt,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for StartNixDaemonReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
