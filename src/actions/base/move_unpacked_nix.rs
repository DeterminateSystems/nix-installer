use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, ActionState, Action};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MoveUnpackedNix {
    source: PathBuf,
}

impl MoveUnpackedNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(source: PathBuf) -> Result<Self, HarmonicError> {
        // Note: Do NOT try to check for the source/dest since the installer creates those
        Ok(Self { source })
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<MoveUnpackedNix> {
    type Error = MoveUnpackedNixError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { source } = &self;
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
        let Self { source } = self;

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
            .map_err(|e| HarmonicError::Rename(src, dest.to_owned(), e))?;

        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<MoveUnpackedNix>> for ActionState<Action> {
    fn from(v: ActionState<MoveUnpackedNix>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::MoveUnpackedNix(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::MoveUnpackedNix(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::MoveUnpackedNix(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum MoveUnpackedNixError {

}
