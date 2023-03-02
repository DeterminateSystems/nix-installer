use std::path::PathBuf;

use bytes::{Buf, Bytes};
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
    pub fn typetag() -> &'static str {
        "fetch_and_unpack_nix"
    }
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(url: Url, dest: PathBuf) -> Result<StatefulAction<Self>, ActionError> {
        // TODO(@hoverbear): Check URL exists?
        // TODO(@hoverbear): Check tempdir exists

        match url.scheme() {
            "https" | "http" | "file" => (),
            _ => {
                return Err(ActionError::Custom(Box::new(
                    FetchUrlError::UnknownUrlScheme,
                )))
            },
        };

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

        let bytes = match url.scheme() {
            "https" | "http" => {
                let res = reqwest::get(url.clone())
                    .await
                    .map_err(|e| ActionError::Custom(Box::new(FetchUrlError::Reqwest(e))))?;
                res.bytes()
                    .await
                    .map_err(|e| ActionError::Custom(Box::new(FetchUrlError::Reqwest(e))))?
            },
            "file" => {
                let buf = tokio::fs::read(url.path())
                    .await
                    .map_err(|e| ActionError::Read(PathBuf::from(url.path()), e))?;
                Bytes::from(buf)
            },
            _ => {
                return Err(ActionError::Custom(Box::new(
                    FetchUrlError::UnknownUrlScheme,
                )))
            },
        };

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
    #[error("Unknown url scheme")]
    UnknownUrlScheme,
}
