use std::path::{PathBuf};

use bytes::Buf;
use reqwest::Url;
use tokio::task::spawn_blocking;

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchNix {
    url: Url,
    destination: PathBuf,
}

impl FetchNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(url: Url, destination: PathBuf) -> Result<Self, HarmonicError> {
        // TODO(@hoverbear): Check URL exists?
        // TODO(@hoverbear): Check tempdir exists

        Ok(Self { url, destination })
    }
}

#[async_trait::async_trait]
impl Actionable for FetchNix {
    type Receipt = FetchNixReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { url, destination } = &self;
        vec![ActionDescription::new(
            format!("Fetch Nix from `{url}`"),
            vec![format!(
                "Unpack it to `{}` (moved later)",
                destination.display()
            )],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { url, destination } = self;

        tracing::trace!(%url, "Fetching url");
        let res = reqwest::get(url.clone())
            .await
            .map_err(HarmonicError::Reqwest)?;
        let bytes = res.bytes().await.map_err(HarmonicError::Reqwest)?;
        // TODO(@Hoverbear): Pick directory
        tracing::trace!("Unpacking tar.xz");
        let destination_clone = destination.clone();
        let handle: Result<(), HarmonicError> = spawn_blocking(move || {
            let decoder = xz2::read::XzDecoder::new(bytes.reader());
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&destination_clone).map_err(HarmonicError::Unarchive)?;
            tracing::debug!(destination = %destination_clone.display(), "Downloaded & extracted Nix");
            Ok(())
        })
        .await?;

        handle?;

        Ok(FetchNixReceipt { url, destination })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchNixReceipt {
    url: Url,
    destination: PathBuf,
}

#[async_trait::async_trait]
impl Revertable for FetchNixReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
