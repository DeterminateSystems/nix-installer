use crate::HarmonicError;

use super::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixDaemonService {}

impl ConfigureNixDaemonService {
    pub fn plan() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for ConfigureNixDaemonService {
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Start the systemd Nix daemon".to_string(),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            ),
        ]
    }

    async fn execute(self) -> Result<ActionReceipt, HarmonicError> {
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
