use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixTreeDirs {}

impl CreateNixTreeDirs {
    pub fn plan(name: String, uid: usize) -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateNixTreeDirs {
    type Receipt = CreateNixTreeDirsReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Create a directory tree in `/nix`"),
            vec![format!(
                "Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"
            )],
        )]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self {} = self;
        Ok(CreateNixTreeDirsReceipt {})
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixTreeDirsReceipt {}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateNixTreeDirsReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
