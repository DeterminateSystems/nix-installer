use std::path::PathBuf;

use bytes::Buf;
use reqwest::Url;

use tokio::task::JoinError;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

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
#[typetag::serde(name = "fetch_nix")]
impl Action for FetchNix {
    fn tracing_synopsis(&self) -> String {
        format!("Fetch Nix from `{}`", self.url)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!(
                "Unpack it to `{}` (moved later)",
                self.dest.display()
            )],
        )]
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

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![/* Deliberately empty -- this is a noop */]
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

        Ok(())
    }

    fn action_state(&self) -> ActionState {
        self.action_state
    }

    fn set_action_state(&mut self, action_state: ActionState) {
        self.action_state = action_state;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FetchNixError {
    #[error("Joining spawned async task")]
    Join(
        #[source]
        #[from]
        JoinError,
    ),
    #[error("Request error")]
    Reqwest(
        #[from]
        #[source]
        reqwest::Error,
    ),
    #[error("Unarchiving error")]
    Unarchive(#[source] std::io::Error),
}
