use tokio::process::Command;

use crate::{HarmonicError, execute_command};

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartSystemdUnit {
    unit: String,
}

impl StartSystemdUnit {
    pub async fn plan(unit: String) -> Result<Self, HarmonicError> {
        Ok(Self { unit })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for StartSystemdUnit {
    type Receipt = StartSystemdUnitReceipt;
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

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { unit } = self;
        // TODO(@Hoverbear): Handle proxy vars
        

        execute_command(
            Command::new("systemctl")
                .arg("enable")
                .arg(format!("{unit}")),
            false,
        )
        .await?;
        
        execute_command(
            Command::new("systemctl")
                .arg("restart")
                .arg(format!("{unit}")),
            false,
        )
        .await?;

        Ok(Self::Receipt {})
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartSystemdUnitReceipt {}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for StartSystemdUnitReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
