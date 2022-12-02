use std::path::{Path, PathBuf};

use crate::action::{Action, ActionDescription, ActionError, StatefulAction};

const DEST: &str = "/nix/store";

/**
Move an unpacked Nix at `src` to `/nix`
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MoveUnpackedNix {
    src: PathBuf,
}

impl MoveUnpackedNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(src: PathBuf) -> Result<StatefulAction<Self>, ActionError> {
        // Note: Do NOT try to check for the src/dest since the installer creates those
        Ok(Self { src }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "mount_unpacked_nix")]
impl Action for MoveUnpackedNix {
    fn tracing_synopsis(&self) -> String {
        "Move the downloaded Nix into `/nix`".to_string()
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Move the downloaded Nix into `/nix`"),
            vec![format!(
                "Nix is being downloaded to `{}` and should be in `nix`",
                self.src.display(),
            )],
        )]
    }

    #[tracing::instrument(skip_all, fields(
        src = %self.src.display(),
        dest = DEST,
    ))]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { src } = self;

        // TODO(@Hoverbear): I would like to make this less awful
        let found_nix_paths = glob::glob(&format!("{}/nix-*", src.display()))
            .map_err(|e| ActionError::Custom(Box::new(e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ActionError::Custom(Box::new(e)))?;
        assert_eq!(
            found_nix_paths.len(),
            1,
            "Did not expect to find multiple nix paths, please report this"
        );
        let found_nix_path = found_nix_paths.into_iter().next().unwrap();
        let src_store = found_nix_path.join("store");
        let dest = Path::new(DEST);
        tracing::trace!(src = %src_store.display(), dest = %dest.display(), "Renaming");
        tokio::fs::rename(src_store.clone(), dest)
            .await
            .map_err(|e| ActionError::Rename(src_store.clone(), dest.to_owned(), e))?;

        tokio::fs::remove_dir_all(src)
            .await
            .map_err(|e| ActionError::Rename(src_store, dest.to_owned(), e))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![/* Deliberately empty -- this is a noop */]
    }

    #[tracing::instrument(skip_all, fields(
        src = %self.src.display(),
        dest = DEST,
    ))]
    async fn revert(&mut self) -> Result<(), ActionError> {
        // Noop
        Ok(())
    }
}

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
