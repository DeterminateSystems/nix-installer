use crate::{HarmonicError, InstallSettings};

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemon {}

impl StartNixDaemon {
    pub fn plan(settings: InstallSettings) -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for StartNixDaemon {
    type Receipt = StartNixDaemonReceipt;
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

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        todo!()
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemonReceipt {}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for StartNixDaemonReceipt {
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
