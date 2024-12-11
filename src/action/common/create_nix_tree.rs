use tracing::{span, Span};

use crate::action::base::CreateDirectory;
use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

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
#[serde(tag = "action_name", rename = "create_nix_tree")]
pub struct CreateNixTree {
    create_directories: Vec<StatefulAction<CreateDirectory>>,
}

impl CreateNixTree {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let mut create_directories = Vec::default();
        for path in PATHS {
            // We use `create_dir` over `create_dir_all` to ensure we always set permissions right
            create_directories.push(
                CreateDirectory::plan(path, String::from("root"), None, 0o0755, true)
                    .await
                    .map_err(Self::error)?,
            )
        }

        Ok(Self { create_directories }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_nix_tree")]
impl Action for CreateNixTree {
    fn action_tag() -> ActionTag {
        ActionTag("create_nix_tree")
    }
    fn tracing_synopsis(&self) -> String {
        "Create a directory tree in `/nix`".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "create_nix_tree",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let Self { create_directories } = &self;

        let mut create_directory_descriptions = Vec::new();
        for create_directory in create_directories {
            if let Some(val) = create_directory.describe_execute().first() {
                create_directory_descriptions.push(val.description.clone())
            }
        }
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            create_directory_descriptions,
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // Just do sequential since parallelizing this will have little benefit
        for create_directory in self.create_directories.iter_mut() {
            create_directory.try_execute().await.map_err(Self::error)?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Remove the directory tree in `/nix`".to_string(),
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
        let mut errors = vec![];
        // Just do sequential since parallelizing this will have little benefit
        for create_directory in self.create_directories.iter_mut().rev() {
            if let Err(err) = create_directory.try_revert().await {
                errors.push(err);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(errors
                .into_iter()
                .next()
                .expect("Expected 1 len Vec to have at least 1 item"))
        } else {
            Err(Self::error(ActionErrorKind::MultipleChildren(errors)))
        }
    }
}
