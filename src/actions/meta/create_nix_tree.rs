use serde::Serialize;

use crate::HarmonicError;

use crate::actions::base::{CreateDirectory, CreateDirectoryError};
use crate::actions::{ActionDescription, Actionable, ActionState, Action};

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
impl Actionable for ActionState<CreateNixTree> {
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
    async fn execute(&mut self) -> Result<(), HarmonicError> {
        let Self { create_directories } = self;

        let mut successes = Vec::with_capacity(create_directories.len());
        // Just do sequential since parallizing this will have little benefit
        for create_directory in create_directories {
            create_directory.execute().await?
        }

        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<CreateNixTree>> for ActionState<Action> {
    fn from(v: ActionState<CreateNixTree>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::CreateNixTree(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::CreateNixTree(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::CreateNixTree(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateNixTreeError {

}
