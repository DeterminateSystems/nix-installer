use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixDaemonService {}

impl ConfigureNixDaemonService {
    pub fn plan() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for ConfigureNixDaemonService {
    type Receipt = ConfigureNixDaemonServiceReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Configure the Nix Daemon Service".to_string(),
                vec![
                    "Sets init system specific options".to_string()
                ]
            ),
        ]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        todo!()
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixDaemonServiceReceipt {}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for ConfigureNixDaemonServiceReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Stop the systemd Nix daemon".to_string(),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            ),
        ]
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
