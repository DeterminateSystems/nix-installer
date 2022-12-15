use crate::{
    action::{ActionError, StatefulAction},
    execute_command, set_env,
};

use glob::glob;

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{Action, ActionDescription};

/**
Setup the default Nix profile with `nss-cacert` and `nix` itself.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SetupDefaultProfile {
    channels: Vec<String>,
}

impl SetupDefaultProfile {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(channels: Vec<String>) -> Result<StatefulAction<Self>, ActionError> {
        Ok(Self { channels }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "setup_default_profile")]
impl Action for SetupDefaultProfile {
    fn tracing_synopsis(&self) -> String {
        "Setup the default Nix profile".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "setup_default_profile",
            channels = self.channels.join(","),
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { channels } = self;

        // Find an `nix` package
        let nix_pkg_glob = "/nix/store/*-nix-*";
        let mut found_nix_pkg = None;
        for entry in glob(nix_pkg_glob).map_err(|e| {
            ActionError::Custom(Box::new(SetupDefaultProfileError::GlobPatternError(e)))
        })? {
            match entry {
                Ok(path) => {
                    // TODO(@Hoverbear): Should probably ensure is unique
                    found_nix_pkg = Some(path);
                    break;
                },
                Err(_) => continue, /* Ignore it */
            };
        }
        let nix_pkg = if let Some(nix_pkg) = found_nix_pkg {
            nix_pkg
        } else {
            return Err(ActionError::Custom(Box::new(
                SetupDefaultProfileError::NoNix,
            )));
        };

        // Find an `nss-cacert` package, add it too.
        let nss_ca_cert_pkg_glob = "/nix/store/*-nss-cacert-*";
        let mut found_nss_ca_cert_pkg = None;
        for entry in glob(nss_ca_cert_pkg_glob).map_err(|e| {
            ActionError::Custom(Box::new(SetupDefaultProfileError::GlobPatternError(e)))
        })? {
            match entry {
                Ok(path) => {
                    // TODO(@Hoverbear): Should probably ensure is unique
                    found_nss_ca_cert_pkg = Some(path);
                    break;
                },
                Err(_) => continue, /* Ignore it */
            };
        }
        let nss_ca_cert_pkg = if let Some(nss_ca_cert_pkg) = found_nss_ca_cert_pkg {
            nss_ca_cert_pkg
        } else {
            return Err(ActionError::Custom(Box::new(
                SetupDefaultProfileError::NoNssCacert,
            )));
        };

        // Install `nix` itself into the store
        execute_command(
            Command::new(nix_pkg.join("bin/nix-env"))
                .process_group(0)
                .arg("-i")
                .arg(&nix_pkg)
                .arg("-i")
                .arg(&nss_ca_cert_pkg)
                .stdin(std::process::Stdio::null())
                .env(
                    "HOME",
                    dirs::home_dir().ok_or_else(|| {
                        ActionError::Custom(Box::new(SetupDefaultProfileError::NoRootHome))
                    })?,
                )
                .env(
                    "NIX_SSL_CERT_FILE",
                    nss_ca_cert_pkg.join("etc/ssl/certs/ca-bundle.crt"),
                ), /* This is apparently load bearing... */
        )
        .await
        .map_err(|e| ActionError::Command(e))?;

        // Install `nss-cacert` into the store
        // execute_command(
        //     Command::new(nix_pkg.join("bin/nix-env"))
        //         .arg("-i")
        //         .arg(&nss_ca_cert_pkg)
        //         .env(
        //             "NIX_SSL_CERT_FILE",
        //             nss_ca_cert_pkg.join("etc/ssl/certs/ca-bundle.crt"),
        //         ),
        // )
        // .await
        // .map_err(|e| SetupDefaultProfileError::Command(e).boxed())?;

        set_env(
            "NIX_SSL_CERT_FILE",
            "/nix/var/nix/profiles/default/etc/ssl/certs/ca-bundle.crt",
        );

        if !channels.is_empty() {
            let mut command = Command::new(nix_pkg.join("bin/nix-channel"));
            command.process_group(0);
            command.arg("--update");
            for channel in channels {
                command.arg(channel);
            }
            command.env(
                "NIX_SSL_CERT_FILE",
                "/nix/var/nix/profiles/default/etc/ssl/certs/ca-bundle.crt",
            );
            command.stdin(std::process::Stdio::null());

            execute_command(&mut command)
                .await
                .map_err(|e| ActionError::Command(e))?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Unset the default Nix profile".to_string(),
            vec!["TODO".to_string()],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        std::env::remove_var("NIX_SSL_CERT_FILE");

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SetupDefaultProfileError {
    #[error("Glob pattern error")]
    GlobPatternError(
        #[from]
        #[source]
        glob::PatternError,
    ),
    #[error("Glob globbing error")]
    GlobGlobError(
        #[from]
        #[source]
        glob::GlobError,
    ),
    #[error("Unarchived Nix store did not appear to include a `nss-cacert` location")]
    NoNssCacert,
    #[error("Unarchived Nix store did not appear to include a `nix` location")]
    NoNix,
    #[error("No root home found to place channel configuration in")]
    NoRootHome,
}
