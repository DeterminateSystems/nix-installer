use std::path::{PathBuf};

use bytes::Buf;
use reqwest::Url;
use serde::Serialize;
use tokio::task::spawn_blocking;

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, ActionState, Action};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchNix {
    url: Url,
    destination: PathBuf,
}

impl FetchNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(url: Url, destination: PathBuf) -> Result<Self, FetchNixError> {
        // TODO(@hoverbear): Check URL exists?
        // TODO(@hoverbear): Check tempdir exists

        Ok(Self { url, destination })
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<FetchNix> {
    type Error = FetchNixError;
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
    async fn execute(&mut self) -> Result<(), Self::Error> {
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

        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<FetchNix>> for ActionState<Action> {
    fn from(v: ActionState<FetchNix>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::FetchNix(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::FetchNix(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::FetchNix(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum FetchNixError {

}
