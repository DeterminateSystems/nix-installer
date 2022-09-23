use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, ActionState, Action, ActionError};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MoveUnpackedNix {
    source: PathBuf,
    action_state: ActionState,
}

impl MoveUnpackedNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(source: PathBuf) -> Result<Self, MoveUnpackedNixError> {
        // Note: Do NOT try to check for the source/dest since the installer creates those
        Ok(Self { source, action_state: ActionState::Planned })
    }
}

#[async_trait::async_trait]
impl Actionable for MoveUnpackedNix {
    type Error = MoveUnpackedNixError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { source, action_state: _ } = &self;
        vec![ActionDescription::new(
            format!("Move the downloaded Nix into `/nix`"),
            vec![format!(
                "Nix is being downloaded to `{}` and should be in `nix`",
                source.display(),
            )],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self { source, action_state } = self;

        // TODO(@Hoverbear): I would like to make this less awful
        let found_nix_paths =
            glob::glob(&format!("{}/nix-*", source.display()))?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            found_nix_paths.len(),
            1,
            "Did not expect to find multiple nix paths, please report this"
        );
        let found_nix_path = found_nix_paths.into_iter().next().unwrap();
        tracing::trace!("Renaming");
        let src = found_nix_path.join("store");
        let dest = Path::new("/nix/store");
        tokio::fs::rename(src.clone(), dest)
            .await
            .map_err(|e| MoveUnpackedNixError::Rename(src, dest.to_owned(), e))?;

        *action_state = ActionState::Completed;
        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

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
    GlobPatternError(#[from] #[source] #[serde(serialize_with = "crate::serialize_error_to_display")] glob::PatternError),
    #[error("Glob globbing error")]
    GlobGlobError(#[from] #[source] #[serde(serialize_with = "crate::serialize_error_to_display")] glob::GlobError),
    #[error("Rename `{0}` to `{1}`")]
    Rename(std::path::PathBuf, std::path::PathBuf, #[source] #[serde(serialize_with = "crate::serialize_error_to_display")] std::io::Error),
}
