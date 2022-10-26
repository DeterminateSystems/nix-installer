use crate::action::base::{CreateDirectory, CreateDirectoryError};
use crate::action::{Action, ActionDescription, ActionState};

const PATHS: &[&str] = &[
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
    action_state: ActionState,
}

impl CreateNixTree {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut create_directories = Vec::default();
        for path in PATHS {
            // We use `create_dir` over `create_dir_all` to ensure we always set permissions right
            create_directories.push(CreateDirectory::plan(path, None, None, 0o0755, false).await?)
        }

        Ok(Self {
            create_directories,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "creat-nix-tree")]
impl Action for CreateNixTree {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Create a directory tree in `/nix`"),
                vec![
                    format!(
                        "Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"
                    ),
                    format!(
                        "Creates: {}",
                        PATHS
                            .iter()
                            .map(|v| format!("`{v}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ],
            )]
        }
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            create_directories,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating nix tree");
            return Ok(());
        }
        *action_state = ActionState::Progress;
        tracing::debug!("Creating nix tree");

        // Just do sequential since parallelizing this will have little benefit
        for create_directory in create_directories {
            create_directory.execute().await?
        }

        tracing::trace!("Created nix tree");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Remove the directory tree in `/nix`"),
                vec![
                    format!(
                        "Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"
                    ),
                    format!(
                        "Removes: {}",
                        PATHS
                            .iter()
                            .rev()
                            .map(|v| format!("`{v}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ],
            )]
        }
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            create_directories,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Deleting nix tree");
            return Ok(());
        }
        *action_state = ActionState::Progress;
        tracing::debug!("Deleting nix tree");

        // Just do sequential since parallelizing this will have little benefit
        for create_directory in create_directories.iter_mut().rev() {
            create_directory.revert().await?
        }

        tracing::trace!("Deleted nix tree");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateNixTreeError {
    #[error("Creating directory")]
    CreateDirectory(
        #[source]
        #[from]
        CreateDirectoryError,
    ),
}
