use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

const DEST: &str = "/nix/store";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MoveUnpackedNix {
    src: PathBuf,
    action_state: ActionState,
}

impl MoveUnpackedNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(src: PathBuf) -> Result<Self, MoveUnpackedNixError> {
        // Note: Do NOT try to check for the src/dest since the installer creates those
        Ok(Self {
            src,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for MoveUnpackedNix {
    type Error = MoveUnpackedNixError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Move the downloaded Nix into `/nix`"),
                vec![format!(
                    "Nix is being downloaded to `{}` and should be in `nix`",
                    self.src.display(),
                )],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        src = %self.src.display(),
        dest = DEST,
    ))]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self { src, action_state } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Moving Nix");
            return Ok(());
        }
        tracing::debug!("Moving Nix");

        // TODO(@Hoverbear): I would like to make this less awful
        let found_nix_paths =
            glob::glob(&format!("{}/nix-*", src.display()))?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            found_nix_paths.len(),
            1,
            "Did not expect to find multiple nix paths, please report this"
        );
        let found_nix_path = found_nix_paths.into_iter().next().unwrap();
        tracing::trace!("Renaming");
        let src_store = found_nix_path.join("store");
        let dest = Path::new(DEST);
        tokio::fs::rename(src_store.clone(), dest)
            .await
            .map_err(|e| MoveUnpackedNixError::Rename(src_store.clone(), dest.to_owned(), e))?;

        tokio::fs::remove_dir_all(src)
            .await
            .map_err(|e| MoveUnpackedNixError::Rename(src_store, dest.to_owned(), e))?;
        tracing::trace!("Moved Nix");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![/* Deliberately empty -- this is a noop */]
        }
    }

    #[tracing::instrument(skip_all, fields(
        src = %self.src.display(),
        dest = DEST,
    ))]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            src: _,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Unmove Nix (noop)");
            return Ok(());
        }
        tracing::debug!("Unmove Nix (noop)");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }
}

impl From<MoveUnpackedNix> for Action {
    fn from(v: MoveUnpackedNix) -> Self {
        Action::MoveUnpackedNix(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum MoveUnpackedNixError {
    #[error("Glob pattern error")]
    GlobPatternError(
        #[from]
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        glob::PatternError,
    ),
    #[error("Glob globbing error")]
    GlobGlobError(
        #[from]
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        glob::GlobError,
    ),
    #[error("Rename `{0}` to `{1}`")]
    Rename(
        std::path::PathBuf,
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
