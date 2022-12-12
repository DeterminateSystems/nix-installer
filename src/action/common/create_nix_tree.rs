use crate::action::base::CreateDirectory;
use crate::action::{Action, ActionDescription, ActionError, StatefulAction};

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

/**
Create the `/nix` tree
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixTree {
    create_directories: Vec<StatefulAction<CreateDirectory>>,
}

impl CreateNixTree {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let mut create_directories = Vec::default();
        for path in PATHS {
            // We use `create_dir` over `create_dir_all` to ensure we always set permissions right
            create_directories.push(CreateDirectory::plan(path, None, None, 0o0755, false).await?)
        }

        Ok(Self { create_directories }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_nix_tree")]
impl Action for CreateNixTree {
    fn tracing_synopsis(&self) -> String {
        "Create a directory tree in `/nix`".to_string()
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
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

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { create_directories } = self;

        // Just do sequential since parallelizing this will have little benefit
        for create_directory in create_directories {
            create_directory.try_execute().await?
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
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

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self { create_directories } = self;

        // Just do sequential since parallelizing this will have little benefit
        for create_directory in create_directories.iter_mut().rev() {
            create_directory.try_revert().await?
        }

        Ok(())
    }
}
