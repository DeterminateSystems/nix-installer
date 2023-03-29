use std::path::{Path, PathBuf};

use tracing::{span, Span};

use crate::action::{Action, ActionDescription, ActionError, ActionTag, StatefulAction};

pub(crate) const DEST: &str = "/nix/";

/**
Move an unpacked Nix at `src` to `/nix`
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MoveUnpackedNix {
    unpacked_path: PathBuf,
}

impl MoveUnpackedNix {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(unpacked_path: PathBuf) -> Result<StatefulAction<Self>, ActionError> {
        // Note: Do NOT try to check for the src/dest since the installer creates those
        Ok(Self { unpacked_path }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "mount_unpacked_nix")]
impl Action for MoveUnpackedNix {
    fn action_tag() -> ActionTag {
        ActionTag("move_unpacked_nix")
    }
    fn tracing_synopsis(&self) -> String {
        "Move the downloaded Nix into `/nix`".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "mount_unpacked_nix",
            src = tracing::field::display(self.unpacked_path.display()),
            dest = DEST,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Move the downloaded Nix into `/nix`"),
            vec![format!(
                "Nix is being downloaded to `{}` and should be in `/nix`",
                self.unpacked_path.display(),
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { unpacked_path } = self;

        // This is the `nix-$VERSION` folder which unpacks from the tarball, not a nix derivation
        let found_nix_paths = glob::glob(&format!("{}/nix-*", unpacked_path.display()))
            .map_err(|e| ActionError::Custom(Box::new(e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ActionError::Custom(Box::new(e)))?;
        if found_nix_paths.len() != 1 {
            return Err(ActionError::MalformedBinaryTarball);
        }
        let found_nix_path = found_nix_paths.into_iter().next().unwrap();
        let src_store = found_nix_path.join("store");
        let mut src_store_listing = tokio::fs::read_dir(src_store.clone())
            .await
            .map_err(|e| ActionError::ReadDir(src_store.clone(), e))?;
        let dest_store = Path::new(DEST).join("store");
        if dest_store.exists() {
            if !dest_store.is_dir() {
                return Err(ActionError::PathWasNotDirectory(dest_store.clone()))?;
            }
        } else {
            tokio::fs::create_dir(&dest_store)
                .await
                .map_err(|e| ActionError::CreateDirectory(dest_store.clone(), e))?;
        }

        while let Some(entry) = src_store_listing
            .next_entry()
            .await
            .map_err(|e| ActionError::ReadDir(src_store.clone(), e))?
        {
            let entry_dest = dest_store.join(entry.file_name());
            if entry_dest.exists() {
                tracing::trace!(src = %entry.path().display(), dest = %entry_dest.display(), "Skipping, already exists");
            } else {
                tracing::trace!(src = %entry.path().display(), dest = %entry_dest.display(), "Renaming");
                tokio::fs::rename(&entry.path(), &entry_dest)
                    .await
                    .map_err(|e| {
                        ActionError::Rename(entry.path().clone(), entry_dest.to_owned(), e)
                    })?;
            }
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![/* Deliberately empty -- this is a noop */]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), Vec<ActionError>> {
        // Noop
        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum MoveUnpackedNixError {
    #[error("Glob pattern error")]
    GlobPatternError(
        #[from]
        #[source]
        glob::PatternError,
    ),
    #[error("Glob globbing error")]
    GlobGlobError(
        #[from]
        #[source]
        glob::GlobError,
    ),
}
