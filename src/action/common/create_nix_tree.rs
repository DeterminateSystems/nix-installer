use std::os::unix::fs::MetadataExt;

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
                CreateDirectory::plan(path, None, None, 0o0755, true)
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
        vec![
            ActionDescription::new(self.tracing_synopsis(), create_directory_descriptions),
            ActionDescription::new(
                "Synchronize /nix/var ownership".to_string(),
                vec![format!(
                    "Will update existing files in /nix/var to be owned by User ID 0, Group ID 0"
                )],
            ),
        ]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // Just do sequential since parallelizing this will have little benefit
        for create_directory in self.create_directories.iter_mut() {
            create_directory.try_execute().await.map_err(Self::error)?;
        }

        ensure_nix_var_ownership().await.map_err(Self::error)?;

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

/// Everything under /nix/var (with two deprecated exceptions below) should be owned by 0:0.
///
/// * /nix/var/nix/profiles/per-user/*
/// * /nix/var/nix/gcroots/per-user/*
///
/// This function walks /nix/var and makes sure that is true.
async fn ensure_nix_var_ownership() -> Result<(), ActionErrorKind> {
    let entryiter = walkdir::WalkDir::new("/nix/var")
        .follow_links(false)
        .same_file_system(true)
        .contents_first(true)
        .into_iter()
        .filter_entry(|entry| {
            let parent = entry.path().parent();

            if parent == Some(std::path::Path::new("/nix/var/nix/profiles/per-user"))
                || parent == Some(std::path::Path::new("/nix/var/nix/gcroots/per-user"))
            {
                // False means do *not* descend into this directory
                // ...which we don't want to do, because the per-user subdirectories are usually owned by that user.
                return false;
            }

            true
        })
        .filter_map(|entry| match entry {
            Ok(entry) => Some(entry),
            Err(e) => {
                tracing::warn!(%e, "Failed to get entry in /nix/var");
                None
            },
        })
        .filter_map(|entry| match entry.metadata() {
            Ok(metadata) => Some((entry, metadata)),
            Err(e) => {
                tracing::warn!(
                    path = %entry.path().to_string_lossy(),
                    %e,
                    "Failed to read ownership and mode data"
                );
                None
            },
        })
        .filter_map(|(entry, metadata)| {
            // Dirents that are already 0:0 are to be skipped
            if metadata.uid() == 0 && metadata.gid() == 0 {
                return None;
            }

            Some((entry, metadata))
        });
    for (entry, _metadata) in entryiter {
        tracing::debug!(
            path = %entry.path().to_string_lossy(),
            "Re-owning path to 0:0"
        );

        if let Err(e) = std::os::unix::fs::lchown(entry.path(), Some(0), Some(0)) {
            tracing::warn!(
                path = %entry.path().to_string_lossy(),
                %e,
                "Failed to set the owner:group to 0:0"
            );
        }
    }
    Ok(())
}
