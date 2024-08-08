use std::path::{Path, PathBuf};

use crate::{
    action::{
        base::SetupDefaultProfile,
        common::{ConfigureShellProfile, PlaceNixConfiguration},
        Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
    },
    planner::ShellProfileLocations,
    settings::{CommonSettings, SCRATCH_DIR},
};
use glob::glob;

use tracing::{span, Instrument, Span};

/**
Configure Nix and start it
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "configure_nix")]
pub struct ConfigureNix {
    setup_default_profile: StatefulAction<SetupDefaultProfile>,
    configure_shell_profile: Option<StatefulAction<ConfigureShellProfile>>,
    place_nix_configuration: StatefulAction<PlaceNixConfiguration>,
}

impl ConfigureNix {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        shell_profile_locations: ShellProfileLocations,
        settings: &CommonSettings,
        extra_internal_conf: Option<nix_config_parser::NixConfig>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let setup_default_profile = SetupDefaultProfile::plan(PathBuf::from(SCRATCH_DIR))
            .await
            .map_err(Self::error)?;

        let configure_shell_profile = if settings.modify_profile {
            Some(
                ConfigureShellProfile::plan(shell_profile_locations)
                    .await
                    .map_err(Self::error)?,
            )
        } else {
            None
        };
        let place_nix_configuration = PlaceNixConfiguration::plan(
            settings.nix_build_group_name.clone(),
            settings.proxy.clone(),
            settings.ssl_cert_file.clone(),
            extra_internal_conf.clone(),
            settings.extra_conf.clone(),
            settings.force,
        )
        .await
        .map_err(Self::error)?;

        Ok(Self {
            place_nix_configuration,
            setup_default_profile,
            configure_shell_profile,
        }
        .into())
    }

    pub async fn find_nix_and_ca_cert(
        unpacked_path: &Path,
    ) -> Result<(PathBuf, PathBuf), ActionError> {
        // Find a `nix` package
        let nix_pkg_glob = format!("{}/nix-*/store/*-nix-*.*.*", unpacked_path.display());
        let mut found_nix_pkg = None;
        for entry in glob(&nix_pkg_glob).map_err(Self::error)? {
            match entry {
                Ok(path) => {
                    // If we are curing, the user may have multiple of these installed
                    if let Some(_existing) = found_nix_pkg {
                        return Err(Self::error(ConfigureNixError::MultipleNixPackages))?;
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
            return Err(Self::error(ConfigureNixError::NoNix));
        };

        // Find an `nss-cacert` package
        let nss_ca_cert_pkg_glob =
            format!("{}/nix-*/store/*-nss-cacert-*.*", unpacked_path.display());
        let mut found_nss_ca_cert_pkg = None;
        for entry in glob(&nss_ca_cert_pkg_glob).map_err(Self::error)? {
            match entry {
                Ok(path) => {
                    // If we are curing, the user may have multiple of these installed
                    if let Some(_existing) = found_nss_ca_cert_pkg {
                        return Err(Self::error(ConfigureNixError::MultipleNssCaCertPackages))?;
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
            return Err(Self::error(ConfigureNixError::NoNssCacert));
        };

        Ok((nix_pkg, nss_ca_cert_pkg))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "configure_nix")]
impl Action for ConfigureNix {
    fn action_tag() -> ActionTag {
        ActionTag("configure_nix")
    }
    fn tracing_synopsis(&self) -> String {
        "Configure Nix".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "configure_nix",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let Self {
            setup_default_profile,
            place_nix_configuration,
            configure_shell_profile,
        } = &self;

        let mut buf = setup_default_profile.describe_execute();
        buf.append(&mut place_nix_configuration.describe_execute());
        if let Some(configure_shell_profile) = configure_shell_profile {
            buf.append(&mut configure_shell_profile.describe_execute());
        }
        buf
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            setup_default_profile,
            place_nix_configuration,
            configure_shell_profile,
        } = self;

        if let Some(configure_shell_profile) = configure_shell_profile {
            let setup_default_profile_span = tracing::Span::current().clone();
            let (place_nix_configuration_span, configure_shell_profile_span) = (
                setup_default_profile_span.clone(),
                setup_default_profile_span.clone(),
            );
            tokio::try_join!(
                async move {
                    setup_default_profile
                        .try_execute()
                        .instrument(setup_default_profile_span)
                        .await
                        .map_err(Self::error)
                },
                async move {
                    place_nix_configuration
                        .try_execute()
                        .instrument(place_nix_configuration_span)
                        .await
                        .map_err(Self::error)
                },
                async move {
                    configure_shell_profile
                        .try_execute()
                        .instrument(configure_shell_profile_span)
                        .await
                        .map_err(Self::error)
                },
            )?;
        } else {
            let setup_default_profile_span = tracing::Span::current().clone();
            let place_nix_configuration_span = setup_default_profile_span.clone();
            tokio::try_join!(
                async move {
                    setup_default_profile
                        .try_execute()
                        .instrument(setup_default_profile_span)
                        .await
                        .map_err(Self::error)
                },
                async move {
                    place_nix_configuration
                        .try_execute()
                        .instrument(place_nix_configuration_span)
                        .await
                        .map_err(Self::error)
                },
            )?;
        };

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            setup_default_profile,
            place_nix_configuration,
            configure_shell_profile,
        } = &self;

        let mut buf = Vec::default();
        if let Some(configure_shell_profile) = configure_shell_profile {
            buf.append(&mut configure_shell_profile.describe_revert());
        }
        buf.append(&mut place_nix_configuration.describe_revert());
        buf.append(&mut setup_default_profile.describe_revert());

        buf
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];
        if let Some(configure_shell_profile) = &mut self.configure_shell_profile {
            if let Err(err) = configure_shell_profile.try_revert().await {
                errors.push(err);
            }
        }
        if let Err(err) = self.place_nix_configuration.try_revert().await {
            errors.push(err);
        }
        if let Err(err) = self.setup_default_profile.try_revert().await {
            errors.push(err);
        }

        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(errors
                .into_iter()
                .next()
                .expect("Expected 1 len Vec to have at least 1 item"))
        } else {
            Err(Self::error(ActionErrorKind::MultipleChildren(errors)))
        }
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ConfigureNixError {
    #[error("Unarchived Nix store did not appear to include a `nss-cacert` location")]
    NoNssCacert,
    #[error("Unarchived Nix store did not appear to include a `nix` location")]
    NoNix,
    #[error("Unarchived Nix store appears to contain multiple `nss-ca-cert` packages, cannot select one")]
    MultipleNssCaCertPackages,
    #[error("Unarchived Nix store appears to contain multiple `nix` packages, cannot select one")]
    MultipleNixPackages,
}

impl From<ConfigureNixError> for ActionErrorKind {
    fn from(val: ConfigureNixError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}
