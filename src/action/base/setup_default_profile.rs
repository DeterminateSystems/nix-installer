use std::path::PathBuf;

use crate::{
    action::{ActionError, ActionErrorKind, ActionTag, StatefulAction},
    execute_command, set_env,
};

use glob::glob;

use tokio::{io::AsyncWriteExt, process::Command};
use tracing::{span, Span};

use crate::action::{Action, ActionDescription};

/**
Setup the default Nix profile with `nss-cacert` and `nix` itself.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SetupDefaultProfile {
    unpacked_path: PathBuf,
}

impl SetupDefaultProfile {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(unpacked_path: PathBuf) -> Result<StatefulAction<Self>, ActionError> {
        Ok(Self { unpacked_path }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "setup_default_profile")]
impl Action for SetupDefaultProfile {
    fn action_tag() -> ActionTag {
        ActionTag("setup_default_profile")
    }
    fn tracing_synopsis(&self) -> String {
        "Setup the default Nix profile".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "setup_default_profile",
            unpacked_path = %self.unpacked_path.display(),
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // Find an `nix` package
        let nix_pkg_glob = format!("{}/nix-*/store/*-nix-*.*.*", self.unpacked_path.display());
        let mut found_nix_pkg = None;
        for entry in glob(&nix_pkg_glob).map_err(|e| Self::error(e))? {
            match entry {
                Ok(path) => {
                    // If we are curing, the user may have multiple of these installed
                    if let Some(_existing) = found_nix_pkg {
                        return Err(Self::error(SetupDefaultProfileError::MultipleNixPackages))?;
                    } else {
                        found_nix_pkg = Some(path);
                    }
                    break;
                },
                Err(_) => continue, /* Ignore it */
            };
        }
        let nix_pkg = if let Some(nix_pkg) = found_nix_pkg {
            tokio::fs::read_link(&nix_pkg)
                .await
                .map_err(|e| ActionErrorKind::ReadSymlink(nix_pkg, e))
                .map_err(Self::error)?
        } else {
            return Err(Self::error(SetupDefaultProfileError::NoNix));
        };

        // Find an `nss-cacert` package, add it too.
        let nss_ca_cert_pkg_glob = format!(
            "{}/nix-*/store/*-nss-cacert-*.*",
            self.unpacked_path.display()
        );
        let mut found_nss_ca_cert_pkg = None;
        for entry in glob(&nss_ca_cert_pkg_glob).map_err(|e| Self::error(e))? {
            match entry {
                Ok(path) => {
                    // If we are curing, the user may have multiple of these installed
                    if let Some(_existing) = found_nss_ca_cert_pkg {
                        return Err(Self::error(
                            SetupDefaultProfileError::MultipleNssCaCertPackages,
                        ))?;
                    } else {
                        found_nss_ca_cert_pkg = Some(path);
                    }
                    break;
                },
                Err(_) => continue, /* Ignore it */
            };
        }
        let nss_ca_cert_pkg = if let Some(nss_ca_cert_pkg) = found_nss_ca_cert_pkg {
            tokio::fs::read_link(&nss_ca_cert_pkg)
                .await
                .map_err(|e| ActionErrorKind::ReadSymlink(nss_ca_cert_pkg, e))
                .map_err(Self::error)?
        } else {
            return Err(Self::error(SetupDefaultProfileError::NoNssCacert));
        };

        let found_nix_paths = glob::glob(&format!("{}/nix-*", self.unpacked_path.display()))
            .map_err(|e| Self::error(e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Self::error(e))?;
        if found_nix_paths.len() != 1 {
            return Err(Self::error(ActionErrorKind::MalformedBinaryTarball));
        }
        let found_nix_path = found_nix_paths.into_iter().next().unwrap();
        let reginfo_path = PathBuf::from(found_nix_path).join(".reginfo");
        let reginfo = tokio::fs::read(&reginfo_path)
            .await
            .map_err(|e| ActionErrorKind::Read(reginfo_path.to_path_buf(), e))
            .map_err(Self::error)?;
        let mut load_db_command = Command::new(nix_pkg.join("bin/nix-store"));
        load_db_command.process_group(0);
        load_db_command.arg("--load-db");
        load_db_command.stdin(std::process::Stdio::piped());
        load_db_command.stdout(std::process::Stdio::piped());
        load_db_command.stderr(std::process::Stdio::piped());
        load_db_command.env(
            "HOME",
            dirs::home_dir().ok_or_else(|| Self::error(SetupDefaultProfileError::NoRootHome))?,
        );
        tracing::trace!(
            "Executing `{:?}` with stdin from `{}`",
            load_db_command.as_std(),
            reginfo_path.display()
        );
        let mut handle = load_db_command
            .spawn()
            .map_err(|e| ActionErrorKind::command(&load_db_command, e))
            .map_err(Self::error)?;

        let mut stdin = handle.stdin.take().unwrap();
        stdin
            .write_all(&reginfo)
            .await
            .map_err(|e| ActionErrorKind::Write(PathBuf::from("/dev/stdin"), e))
            .map_err(Self::error)?;
        stdin
            .flush()
            .await
            .map_err(|e| ActionErrorKind::Write(PathBuf::from("/dev/stdin"), e))
            .map_err(Self::error)?;
        drop(stdin);
        tracing::trace!(
            "Wrote `{}` to stdin of `nix-store --load-db`",
            reginfo_path.display()
        );

        let output = handle
            .wait_with_output()
            .await
            .map_err(|e| ActionErrorKind::command(&load_db_command, e))
            .map_err(Self::error)?;
        if !output.status.success() {
            return Err(Self::error(ActionErrorKind::command_output(
                &load_db_command,
                output,
            )));
        };

        // Install `nix` itself into the store
        execute_command(
            Command::new(nix_pkg.join("bin/nix-env"))
                .process_group(0)
                .arg("-i")
                .arg(&nix_pkg)
                .stdin(std::process::Stdio::null())
                .env(
                    "HOME",
                    dirs::home_dir()
                        .ok_or_else(|| Self::error(SetupDefaultProfileError::NoRootHome))?,
                )
                .env(
                    "NIX_SSL_CERT_FILE",
                    nss_ca_cert_pkg.join("etc/ssl/certs/ca-bundle.crt"),
                ), /* This is apparently load bearing... */
        )
        .await
        .map_err(Self::error)?;

        // Install `nix` itself into the store
        execute_command(
            Command::new(nix_pkg.join("bin/nix-env"))
                .process_group(0)
                .arg("-i")
                .arg(&nss_ca_cert_pkg)
                .stdin(std::process::Stdio::null())
                .env(
                    "HOME",
                    dirs::home_dir()
                        .ok_or_else(|| Self::error(SetupDefaultProfileError::NoRootHome))?,
                )
                .env(
                    "NIX_SSL_CERT_FILE",
                    nss_ca_cert_pkg.join("etc/ssl/certs/ca-bundle.crt"),
                ), /* This is apparently load bearing... */
        )
        .await
        .map_err(Self::error)?;

        set_env(
            "NIX_SSL_CERT_FILE",
            "/nix/var/nix/profiles/default/etc/ssl/certs/ca-bundle.crt",
        );

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Unset the default Nix profile".to_string(),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        std::env::remove_var("NIX_SSL_CERT_FILE");

        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum SetupDefaultProfileError {
    #[error("Unarchived Nix store did not appear to include a `nss-cacert` location")]
    NoNssCacert,
    #[error("Unarchived Nix store did not appear to include a `nix` location")]
    NoNix,
    #[error("No root home found to place channel configuration in")]
    NoRootHome,
    #[error("Unarchived Nix store appears to contain multiple `nss-ca-cert` packages, cannot select one")]
    MultipleNssCaCertPackages,
    #[error("Unarchived Nix store appears to contain multiple `nix` packages, cannot select one")]
    MultipleNixPackages,
}

impl Into<ActionErrorKind> for SetupDefaultProfileError {
    fn into(self) -> ActionErrorKind {
        ActionErrorKind::Custom(Box::new(self))
    }
}
