use std::path::PathBuf;

use crate::{
    action::{
        base::SetupDefaultProfile,
        common::{ConfigureShellProfile, PlaceNixConfiguration},
        Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
    },
    planner::ShellProfileLocations,
    settings::{CommonSettings, SCRATCH_DIR},
};

use tracing::{span, Instrument, Span};

/**
Configure Nix and start it
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
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
    ) -> Result<StatefulAction<Self>, ActionError> {
        let setup_default_profile = SetupDefaultProfile::plan(PathBuf::from(SCRATCH_DIR))
            .await
            .map_err(Self::error)?;

        let configure_shell_profile = if settings.modify_profile {
            Some(
                ConfigureShellProfile::plan(
                    shell_profile_locations,
                    settings.ssl_cert_file.clone(),
                )
                .await
                .map_err(Self::error)?,
            )
        } else {
            None
        };
        let place_nix_configuration = PlaceNixConfiguration::plan(
            settings.nix_build_group_name.clone(),
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
