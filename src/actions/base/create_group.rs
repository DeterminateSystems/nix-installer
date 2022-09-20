use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateGroup {
    name: String,
    uid: usize,
}

impl CreateGroup {
    pub fn plan(name: String, uid: usize) -> Self {
        Self { name, uid }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateGroup {
    type Receipt = CreateGroupReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let name = &self.name;
        let uid = &self.uid;
        vec![ActionDescription::new(
            format!("Create group {name} with UID {uid}"),
            vec![format!(
                "The nix daemon requires a system user group its system users can be part of"
            )],
        )]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { name, uid } = self;
        Ok(CreateGroupReceipt { name, uid })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateGroupReceipt {
    name: String,
    uid: usize,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateGroupReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
