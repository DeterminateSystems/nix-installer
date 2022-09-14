use crate::{settings::InstallSettings, HarmonicError};

use super::{Actionable, ActionReceipt, Revertable};


#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUser {
    name: String,
    uid: usize,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUserReceipt {
    name: String,
    uid: usize,
}

impl CreateUser {
    pub fn plan(name: String, uid: usize) -> Self {
        Self { name, uid }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateUser {
    fn description(&self) -> String {
        todo!()
    }

    async fn execute(self) -> Result<ActionReceipt, HarmonicError> {
        let Self { name, uid } = self;
        Ok(ActionReceipt::CreateUser(CreateUserReceipt { name, uid }))
    }
}


#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateUserReceipt {
    fn description(&self) -> String {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();
        
        Ok(())
    }
}
