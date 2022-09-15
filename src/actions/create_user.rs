use crate::{settings::InstallSettings, HarmonicError};

use super::{Actionable, ActionReceipt, Revertable, ActionDescription};


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
    fn description(&self) -> Vec<ActionDescription> {
        let name = &self.name;
        let uid = &self.uid;
        vec![
            ActionDescription::new(
                format!("Create user {name} with UID {uid}"),
                vec![
                    format!("The nix daemon requires system users it can act as in order to build"),
                ]
            )
        ]
    }

    async fn execute(self) -> Result<ActionReceipt, HarmonicError> {
        let Self { name, uid } = self;
        Ok(ActionReceipt::CreateUser(CreateUserReceipt { name, uid }))
    }
}


#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateUserReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();
        
        Ok(())
    }
}
