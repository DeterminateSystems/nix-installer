use crate::{HarmonicError, InstallSettings};

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNix {}

impl ConfigureNix {
    pub fn plan(settings: InstallSettings) -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for ConfigureNix {
    type Receipt = ConfigureNixReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Configure the Nix daemon".to_string(),
                vec![
                    "Blah".to_string()
                ]
            ),
        ]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        todo!()
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixReceipt {}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for ConfigureNixReceipt {
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
