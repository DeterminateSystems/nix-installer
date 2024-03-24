use std::path::PathBuf;

use bytes::{Buf, Bytes};
use reqwest::Url;
use tracing::{span, Span};

use crate::{
    action::{Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction},
    parse_ssl_cert,
    settings::UrlOrPath,
};

/**
Fetch a URL to the given path
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct FetchAndUnpackNix {
    url_or_path: Option<UrlOrPath>,
    dest: PathBuf,
    proxy: Option<Url>,
    ssl_cert_file: Option<PathBuf>,
}

impl FetchAndUnpackNix {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        url_or_path: Option<UrlOrPath>,
        dest: PathBuf,
        proxy: Option<Url>,
        ssl_cert_file: Option<PathBuf>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        // TODO(@hoverbear): Check URL exists?
        // TODO(@hoverbear): Check tempdir exists

        if let Some(UrlOrPath::Url(url)) = &url_or_path {
            match url.scheme() {
                "https" | "http" | "file" => (),
                _ => return Err(Self::error(ActionErrorKind::UnknownUrlScheme)),
            }
        }

        if let Some(proxy) = &proxy {
            match proxy.scheme() {
                "https" | "http" | "socks5" => (),
                _ => return Err(Self::error(FetchUrlError::UnknownProxyScheme)),
            };
        }

        if let Some(ssl_cert_file) = &ssl_cert_file {
            parse_ssl_cert(ssl_cert_file).await.map_err(Self::error)?;
        }

        Ok(Self {
            url_or_path,
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
        if let Some(ref url_or_path) = self.url_or_path {
            format!("Fetch `{}` to `{}`", url_or_path, self.dest.display())
        } else {
            "Extract the bundled Nix".to_string()
        }
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "fetch_and_unpack_nix",
            url_or_path = self.url_or_path.as_ref().map(tracing::field::display),
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
        let bytes = match &self.url_or_path {
            &None => Bytes::from(crate::settings::NIX_TARBALL),
            Some(UrlOrPath::Url(url)) => {
                let bytes = match url.scheme() {
                    "https" | "http" => {
                        let mut buildable_client = reqwest::Client::builder();
                        if let Some(proxy) = &self.proxy {
                            buildable_client = buildable_client.proxy(
                                reqwest::Proxy::all(proxy.clone())
                                    .map_err(ActionErrorKind::Reqwest)
                                    .map_err(Self::error)?,
                            )
                        }
                        if let Some(ssl_cert_file) = &self.ssl_cert_file {
                            let ssl_cert =
                                parse_ssl_cert(ssl_cert_file).await.map_err(Self::error)?;
                            buildable_client = buildable_client.add_root_certificate(ssl_cert);
                        }
                        let client = buildable_client
                            .build()
                            .map_err(ActionErrorKind::Reqwest)
                            .map_err(Self::error)?;
                        let req = client
                            .get(url.clone())
                            .build()
                            .map_err(ActionErrorKind::Reqwest)
                            .map_err(Self::error)?;
                        let res = client
                            .execute(req)
                            .await
                            .map_err(ActionErrorKind::Reqwest)
                            .map_err(Self::error)?;
                        res.bytes()
                            .await
                            .map_err(ActionErrorKind::Reqwest)
                            .map_err(Self::error)?
                    },
                    "file" => {
                        let buf = tokio::fs::read(url.path())
                            .await
                            .map_err(|e| ActionErrorKind::Read(PathBuf::from(url.path()), e))
                            .map_err(Self::error)?;
                        Bytes::from(buf)
                    },
                    _ => return Err(Self::error(ActionErrorKind::UnknownUrlScheme)),
                };
                bytes
            },
            Some(UrlOrPath::Path(path)) => {
                let buf = tokio::fs::read(path)
                    .await
                    .map_err(|e| ActionErrorKind::Read(PathBuf::from(path), e))
                    .map_err(Self::error)?;
                Bytes::from(buf)
            },
        };

        // TODO(@Hoverbear): Pick directory
        tracing::trace!("Unpacking tar.xz");
        let dest_clone = self.dest.clone();

        let decoder = xz2::read::XzDecoder::new(bytes.reader());
        let mut archive = tar::Archive::new(decoder);
        archive.set_preserve_permissions(true);
        archive.set_preserve_mtime(true);
        archive.set_unpack_xattrs(true);
        archive
            .unpack(&dest_clone)
            .map_err(FetchUrlError::Unarchive)
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![/* Deliberately empty -- this is a noop */]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum FetchUrlError {
    #[error("Unarchiving error")]
    Unarchive(#[source] std::io::Error),
    #[error("Unknown proxy scheme, `https://`, `socks5://`, and `http://` supported")]
    UnknownProxyScheme,
}

impl From<FetchUrlError> for ActionErrorKind {
    fn from(val: FetchUrlError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}
