use std::path::PathBuf;

use bytes::Buf;
use reqwest::Url;
use tracing::{span, Span};

use crate::action::{Action, ActionDescription, ActionError, StatefulAction};

/**
Fetch a URL to the given path
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchAndUnpackNix {
    url: Url,
    dest: PathBuf,
}

impl FetchAndUnpackNix {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(url: Url, dest: PathBuf) -> Result<StatefulAction<Self>, ActionError> {
        // TODO(@hoverbear): Check URL exists?
        // TODO(@hoverbear): Check tempdir exists

        Ok(Self { url, dest }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "fetch_and_unpack_nix")]
impl Action for FetchAndUnpackNix {
    fn tracing_synopsis(&self) -> String {
        format!("Fetch `{}` to `{}`", self.url, self.dest.display())
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "fetch_and_unpack_nix",
            url = tracing::field::display(&self.url),
            dest = tracing::field::display(self.dest.display()),
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { url, dest } = self;

        let res = reqwest::get(url.clone())
            .await
            .map_err(|e| ActionError::Custom(Box::new(FetchUrlError::Reqwest(e))))?;
        let bytes = res
            .bytes()
            .await
            .map_err(|e| ActionError::Custom(Box::new(FetchUrlError::Reqwest(e))))?;
        // TODO(@Hoverbear): Pick directory
        tracing::trace!("Unpacking tar.xz");
        let dest_clone = dest.clone();

        let decoder = xz2::read::XzDecoder::new(bytes.reader());
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(&dest_clone)
            .map_err(|e| ActionError::Custom(Box::new(FetchUrlError::Unarchive(e))))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![/* Deliberately empty -- this is a noop */]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self { url: _, dest: _ } = self;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FetchUrlError {
    #[error("Request error")]
    Reqwest(
        #[from]
        #[source]
        reqwest::Error,
    ),
    #[error("Unarchiving error")]
    Unarchive(#[source] std::io::Error),
}
