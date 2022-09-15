use crate::{HarmonicError, InstallSettings};

use super::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixTree {
    settings: InstallSettings,
}

impl CreateNixTree {
    pub fn plan(settings: InstallSettings) -> Self {
        Self { settings }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateNixTree {
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Create a directory tree in `/nix`"),
            vec![format!(
                "Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"
            )],
        )]
    }

    async fn execute(self) -> Result<ActionReceipt, HarmonicError> {
        let Self { settings: _ } = self;
        Ok(ActionReceipt::CreateNixTree(CreateNixTreeReceipt {}))
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixTreeReceipt {}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateNixTreeReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
