use std::path::PathBuf;

use bytes::{Buf, Bytes};
use reqwest::Url;
use tracing::{span, Span};

use crate::{
    action::{Action, ActionDescription, ActionError, ActionTag, StatefulAction},
    parse_ssl_cert,
};

/**
Fetch a URL to the given path
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchAndUnpackNix {
    url: Url,
    dest: PathBuf,
    proxy: Option<Url>,
    ssl_cert_file: Option<PathBuf>,
}

impl FetchAndUnpackNix {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        url: Url,
        dest: PathBuf,
        proxy: Option<Url>,
        ssl_cert_file: Option<PathBuf>,
    ) -> Result<StatefulAction<Self>, ActionError> {
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

        if let Some(proxy) = &proxy {
            match proxy.scheme() {
                "https" | "http" | "socks5" => (),
                _ => {
                    return Err(ActionError::Custom(Box::new(
                        FetchUrlError::UnknownProxyScheme,
                    )))
                },
            };
        }

        if let Some(ssl_cert_file) = &ssl_cert_file {
            parse_ssl_cert(&ssl_cert_file).await?;
        }

        Ok(Self {
            url,
            dest,
            proxy,
            ssl_cert_file,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "fetch_and_unpack_nix")]
impl Action for FetchAndUnpackNix {
    fn action_tag() -> ActionTag {
        ActionTag("fetch_and_unpack_nix")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Fetch `{}` to `{}`", self.url, self.dest.display())
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "fetch_and_unpack_nix",
            url = tracing::field::display(&self.url),
            proxy = tracing::field::Empty,
            ssl_cert_file = tracing::field::Empty,
            dest = tracing::field::display(self.dest.display()),
        );
        if let Some(proxy) = &self.proxy {
            span.record("proxy", tracing::field::display(&proxy));
        }
        if let Some(ssl_cert_file) = &self.ssl_cert_file {
            span.record(
                "ssl_cert_file",
                tracing::field::display(&ssl_cert_file.display()),
            );
        }
        span
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let bytes = match self.url.scheme() {
            "https" | "http" => {
                let mut buildable_client = reqwest::Client::builder();
                if let Some(proxy) = &self.proxy {
                    buildable_client =
                        buildable_client.proxy(reqwest::Proxy::all(proxy.clone()).map_err(|e| {
                            ActionError::Custom(Box::new(FetchUrlError::Reqwest(e)))
                        })?)
                }
                if let Some(ssl_cert_file) = &self.ssl_cert_file {
                    let ssl_cert = parse_ssl_cert(&ssl_cert_file).await?;
                    buildable_client = buildable_client.add_root_certificate(ssl_cert);
                }
                let client = buildable_client
                    .build()
                    .map_err(|e| ActionError::Custom(Box::new(FetchUrlError::Reqwest(e))))?;
                let req = client
                    .get(self.url.clone())
                    .build()
                    .map_err(|e| ActionError::Custom(Box::new(FetchUrlError::Reqwest(e))))?;
                let res = client
                    .execute(req)
                    .await
                    .map_err(|e| ActionError::Custom(Box::new(FetchUrlError::Reqwest(e))))?;
                res.bytes()
                    .await
                    .map_err(|e| ActionError::Custom(Box::new(FetchUrlError::Reqwest(e))))?
            },
            "file" => {
                let buf = tokio::fs::read(self.url.path())
                    .await
                    .map_err(|e| ActionError::Read(PathBuf::from(self.url.path()), e))?;
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
        let dest_clone = self.dest.clone();

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
    async fn revert(&mut self) -> Result<(), Vec<ActionError>> {
        Ok(())
    }
}

#[non_exhaustive]
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
    #[error("Unknown url scheme, `file://`, `https://` and `http://` supported")]
    UnknownUrlScheme,
    #[error("Unknown proxy scheme, `https://`, `socks5://`, and `http://` supported")]
    UnknownProxyScheme,
}
