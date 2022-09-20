use std::path::PathBuf;

use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MoveUnpackedNix {
    source: PathBuf,
}

impl MoveUnpackedNix {
    pub fn plan(source: PathBuf) -> Self {
        Self { source, }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for MoveUnpackedNix {
    type Receipt = MoveUnpackedNixReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { source } = &self;
        vec![ActionDescription::new(
            format!("Move the downloaded Nix into `/nix`"),
            vec![format!(
                "Nix is downloaded to `{}` and should be in `nix`", source.display(),
            )],
        )]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { source } = self;
        Ok(MoveUnpackedNixReceipt { })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MoveUnpackedNixReceipt {
    
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for MoveUnpackedNixReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
