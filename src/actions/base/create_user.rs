use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUser {
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
    type Receipt = CreateUserReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let name = &self.name;
        let uid = &self.uid;
        vec![ActionDescription::new(
            format!("Create user {name} with UID {uid}"),
            vec![format!(
                "The nix daemon requires system users it can act as in order to build"
            )],
        )]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { name, uid } = self;
        Ok(CreateUserReceipt { name, uid })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUserReceipt {
    name: String,
    uid: usize,
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
