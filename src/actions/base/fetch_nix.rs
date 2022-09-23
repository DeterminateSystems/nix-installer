use std::path::{PathBuf};

use bytes::Buf;
use reqwest::Url;
use serde::Serialize;
use tokio::task::{spawn_blocking, JoinError};

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, ActionState, Action, ActionError};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchNix {
    url: Url,
    destination: PathBuf,
    action_state: ActionState,
}

impl FetchNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(url: Url, destination: PathBuf) -> Result<Self, FetchNixError> {
        // TODO(@hoverbear): Check URL exists?
        // TODO(@hoverbear): Check tempdir exists

        Ok(Self { url, destination, action_state: ActionState::Planned })
    }
}

#[async_trait::async_trait]
impl Actionable for FetchNix {
    type Error = FetchNixError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { url, destination, action_state: _ } = &self;
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
        let Self { url, destination, action_state } = self;

        tracing::trace!(%url, "Fetching url");
        let res = reqwest::get(url.clone())
            .await
            .map_err(Self::Error::Reqwest)?;
        let bytes = res.bytes().await.map_err(Self::Error::Reqwest)?;
        // TODO(@Hoverbear): Pick directory
        tracing::trace!("Unpacking tar.xz");
        let destination_clone = destination.clone();
        let handle: Result<(), Self::Error> = spawn_blocking(move || {
            let decoder = xz2::read::XzDecoder::new(bytes.reader());
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&destination_clone).map_err(Self::Error::Unarchive)?;
            tracing::debug!(destination = %destination_clone.display(), "Downloaded & extracted Nix");
            Ok(())
        })
        .await?;

        handle?;

        *action_state = ActionState::Completed;
        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<FetchNix> for Action {
    fn from(v: FetchNix) -> Self {
        Action::FetchNix(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum FetchNixError {
    #[error(transparent)]
    Join(
        #[from]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        JoinError
    ),
    #[error("Request error")]
    Reqwest(#[from] #[source]  #[serde(serialize_with = "crate::serialize_error_to_display")] reqwest::Error),
    #[error("Unarchiving error")]
    Unarchive(#[source]  #[serde(serialize_with = "crate::serialize_error_to_display")] std::io::Error),
}
