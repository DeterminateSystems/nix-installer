use std::path::PathBuf;

use bytes::Buf;
use reqwest::Url;
use serde::Serialize;
use tokio::task::JoinError;

use crate::actions::{ActionDescription, ActionError, ActionState, Actionable};

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
#[typetag::serde(name = "fetch-nix")]
impl Actionable for FetchNix {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            url,
            dest,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Fetch Nix from `{url}`"),
                vec![format!("Unpack it to `{}` (moved later)", dest.display())],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        url = %self.url,
        dest = %self.dest.display(),
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
            .map_err(|e| FetchNixError::Reqwest(e).boxed())?;
        let bytes = res
            .bytes()
            .await
            .map_err(|e| FetchNixError::Reqwest(e).boxed())?;
        // TODO(@Hoverbear): Pick directory
        tracing::trace!("Unpacking tar.xz");
        let dest_clone = dest.clone();

        let decoder = xz2::read::XzDecoder::new(bytes.reader());
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(&dest_clone)
            .map_err(|e| FetchNixError::Unarchive(e).boxed())?;

        tracing::trace!("Fetched Nix");
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
        url = %self.url,
        dest = %self.dest.display(),
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

#[derive(Debug, thiserror::Error, Serialize)]
pub enum FetchNixError {
    #[error("Joining spawned async task")]
    Join(
        #[source]
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
