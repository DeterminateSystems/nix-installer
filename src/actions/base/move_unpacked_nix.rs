use std::path::{Path, PathBuf};

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, Revertable};

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
impl<'a> Actionable<'a> for MoveUnpackedNix {
    type Receipt = MoveUnpackedNixReceipt;
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
    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
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

        Ok(MoveUnpackedNixReceipt {})
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MoveUnpackedNixReceipt {}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for MoveUnpackedNixReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
