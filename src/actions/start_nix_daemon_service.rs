use reqwest::redirect::Action;

use crate::{settings::InstallSettings, HarmonicError};

use super::{Actionable, ActionReceipt, Revertable, ActionDescription};


#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemonService {
    
}


#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartNixDaemonServiceReceipt {
}

impl StartNixDaemonService {
    pub fn plan() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for StartNixDaemonService {
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


#[async_trait::async_trait]
impl<'a> Revertable<'a> for StartNixDaemonServiceReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();
        
        Ok(())
    }
}
