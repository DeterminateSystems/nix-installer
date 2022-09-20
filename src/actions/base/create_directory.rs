use std::{
    fs::Permissions,
    path::{Path, PathBuf},
};

use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateDirectory {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
}

impl CreateDirectory {
    pub fn plan(path: impl AsRef<Path>, user: String, group: String, mode: u32) -> Self {
        let path = path.as_ref().to_path_buf();
        Self {
            path,
            user,
            group,
            mode,
        }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateDirectory {
    type Receipt = CreateDirectoryReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Create the directory `/nix`"),
            vec![format!(
                "Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"
            )],
        )]
    }

    async fn execute(self) -> Result<CreateDirectoryReceipt, HarmonicError> {
        let Self {
            path,
            user,
            group,
            mode,
        } = self;
        todo!();
        Ok(CreateDirectoryReceipt {
            path,
            user,
            group,
            mode,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateDirectoryReceipt {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateDirectoryReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Create the directory `/nix`"),
            vec![format!(
                "Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"
            )],
        )]
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        let Self {
            path,
            user,
            group,
            mode,
        } = self;

        todo!();

        Ok(())
    }
}
