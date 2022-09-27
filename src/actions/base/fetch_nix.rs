use std::path::PathBuf;

use bytes::Buf;
use reqwest::Url;
use serde::Serialize;
use tokio::task::{spawn_blocking, JoinError};

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchNix {
    url: Url,
    dest: PathBuf,
    action_state: ActionState,
}

impl FetchNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(url: Url, dest: PathBuf) -> Result<Self, FetchNixError> {
        // TODO(@hoverbear): Check URL exists?
        // TODO(@hoverbear): Check tempdir exists

        Ok(Self {
            url,
            dest,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for FetchNix {
    type Error = FetchNixError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            url,
            dest,
            action_state: _,
        } = &self;
        vec![ActionDescription::new(
            format!("Fetch Nix from `{url}`"),
            vec![format!(
                "Unpack it to `{}` (moved later)",
                dest.display()
            )],
        )]
    }

    #[tracing::instrument(skip_all, fields(
        url = %self.url,
        dest = %self.dest.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            url,
            dest,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Fetching Nix");
            return Ok(());
        }
        tracing::debug!("Fetching Nix");

        let res = reqwest::get(url.clone())
            .await
            .map_err(Self::Error::Reqwest)?;
        let bytes = res.bytes().await.map_err(Self::Error::Reqwest)?;
        // TODO(@Hoverbear): Pick directory
        tracing::trace!("Unpacking tar.xz");
        let dest_clone = dest.clone();
        let handle: Result<(), Self::Error> = spawn_blocking(move || {
            let decoder = xz2::read::XzDecoder::new(bytes.reader());
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&dest_clone).map_err(Self::Error::Unarchive)?;
            tracing::debug!(dest = %dest_clone.display(), "Downloaded & extracted Nix");
            Ok(())
        })
        .await?;

        handle?;

        tracing::trace!("Fetched Nix");
        *action_state = ActionState::Completed;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(
        url = %self.url,
        dest = %self.dest.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            url: _,
            dest: _,
            action_state,
        } = self;

        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Unfetch Nix (noop)");
            return Ok(());
        }
        tracing::debug!("Unfetch Nix (noop)");
        *action_state = ActionState::Uncompleted;
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
        JoinError,
    ),
    #[error("Request error")]
    Reqwest(
        #[from]
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        reqwest::Error,
    ),
    #[error("Unarchiving error")]
    Unarchive(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
}
