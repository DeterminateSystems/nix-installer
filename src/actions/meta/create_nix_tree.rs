use crate::HarmonicError;

use crate::actions::base::{CreateDirectory, CreateDirectoryReceipt};
use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixTree {
    create_directories: Vec<CreateDirectory>,
}

impl CreateNixTree {
    pub async fn plan(force: bool) -> Result<Self, HarmonicError> {
        let mut create_directories = Vec::default();
        let paths = [
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
        for path in paths {
            // We use `create_dir` over `create_dir_all` to ensure we always set permissions right
            create_directories.push(CreateDirectory::plan(path, "root".into(), "root".into(), 0o0755, force).await?)
        }

        Ok(Self { create_directories })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateNixTree {
    type Receipt = CreateNixTreeReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Create a directory tree in `/nix`"),
            vec![format!(
                "Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"
            )],
        )]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { create_directories } = self;
        
        let mut successes = Vec::with_capacity(create_directories.len());
        // Just do sequential since parallizing this will have little benefit
        for create_directory in create_directories {
            successes.push(create_directory.execute().await?)
        }

        Ok(CreateNixTreeReceipt { create_directories: successes })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixTreeReceipt {
    create_directories: Vec<CreateDirectoryReceipt>,
}

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
