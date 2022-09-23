use serde::{Serialize, Deserialize};
use tokio::process::Command;

use crate::error::ActionState;
use crate::{execute_command, HarmonicError};

use crate::actions::{ActionDescription, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartSystemdUnit {
    unit: String,
}

impl StartSystemdUnit {
    #[tracing::instrument(skip_all)]
    pub async fn plan(unit: String) -> Result<Self, HarmonicError> {
        Ok(Self { unit })
    }
}

#[async_trait::async_trait]
impl Actionable for StartSystemdUnit {
    type Receipt = StartSystemdUnitReceipt;
    type Error = StartSystemdUnitError;
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Start the systemd Nix service and socket".to_string(),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            ),
        ]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> ActionState<Self> {
        let Self { unit } = self;
        // TODO(@Hoverbear): Handle proxy vars

        execute_command(
            Command::new("systemctl")
                .arg("enable")
                .arg("--now")
                .arg(format!("{unit}")),
            false,
        )
        .await?;

        Ok(Self::Receipt {})
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartSystemdUnitReceipt {

}

#[async_trait::async_trait]
impl Revertable for StartSystemdUnitReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}


#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
pub enum StartSystemdUnitError {

}


impl ActionState<StartSystemdUnit> {
    fn errored(&self) -> bool {
        false
    }
} 