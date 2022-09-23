use serde::{Serialize, Deserialize};

use crate::HarmonicError;

use crate::actions::base::{CreateDirectory, CreateDirectoryReceipt};
use crate::actions::{ActionDescription, Actionable, Revertable};

const PATHS: &[&str] = &[
    "/nix",
    "/nix/var",
    "/nix/var/log",
    "/nix/var/log/nix",
    "/nix/var/log/nix/drvs",
    "/nix/var/nix",
    "/nix/var/nix/db",
    "/nix/var/nix/gcroots",
    "/nix/var/nix/gcroots/per-user",
    "/nix/var/nix/profiles",
    "/nix/var/nix/profiles/per-user",
    "/nix/var/nix/temproots",
    "/nix/var/nix/userpool",
    "/nix/var/nix/daemon-socket",
];

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixTree {
    create_directories: Vec<CreateDirectory>,
}

impl CreateNixTree {
    #[tracing::instrument(skip_all)]
    pub async fn plan(force: bool) -> Result<Self, HarmonicError> {
        let mut create_directories = Vec::default();
        for path in PATHS {
            // We use `create_dir` over `create_dir_all` to ensure we always set permissions right
            create_directories.push(
                CreateDirectory::plan(path, "root".into(), "root".into(), 0o0755, force).await?,
            )
        }

        Ok(Self { create_directories })
    }
}

#[async_trait::async_trait]
impl Actionable for CreateNixTree {
    type Receipt = CreateNixTreeReceipt;
    type Error = CreateNixTreeError;
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Create a directory tree in `/nix`"),
            vec![
                format!("Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"),
                format!("Creates: {}", PATHS.iter().map(|v| format!("`{v}`")).collect::<Vec<_>>().join(", ")),
            ],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<Self::Receipt, Self::Error> {
        let Self { create_directories } = self;

        let mut successes = Vec::with_capacity(create_directories.len());
        // Just do sequential since parallizing this will have little benefit
        for create_directory in create_directories {
            successes.push(create_directory.execute().await?)
        }

        Ok(CreateNixTreeReceipt {
            create_directories: successes,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixTreeReceipt {
    create_directories: Vec<CreateDirectoryReceipt>,
}

#[async_trait::async_trait]
impl Revertable for CreateNixTreeReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}

#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
pub enum CreateNixTreeError {

}