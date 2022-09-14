use crate::{settings::InstallSettings, HarmonicError};

use super::{Actionable, ActionReceipt, Revertable};


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
    fn description(&self) -> String {
        todo!()
    }

    async fn execute(self) -> Result<ActionReceipt, HarmonicError> {
        todo!()
    }
}


#[async_trait::async_trait]
impl<'a> Revertable<'a> for StartNixDaemonServiceReceipt {
    fn description(&self) -> String {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();
        
        Ok(())
    }
}
