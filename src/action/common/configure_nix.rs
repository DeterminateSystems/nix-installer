use crate::{
    action::{
        base::SetupDefaultProfile,
        common::{ConfigureShellProfile, PlaceChannelConfiguration, PlaceNixConfiguration},
        Action, ActionDescription, ActionError, ActionTag, StatefulAction,
    },
    settings::CommonSettings,
};

use tracing::{span, Instrument, Span};

/**
Configure Nix and start it
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNix {
    setup_default_profile: StatefulAction<SetupDefaultProfile>,
    configure_shell_profile: Option<StatefulAction<ConfigureShellProfile>>,
    place_channel_configuration: StatefulAction<PlaceChannelConfiguration>,
    place_nix_configuration: StatefulAction<PlaceNixConfiguration>,
}

impl ConfigureNix {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(settings: &CommonSettings) -> Result<StatefulAction<Self>, ActionError> {
        let setup_default_profile = SetupDefaultProfile::plan(settings.channels.clone())
            .await
            .map_err(|e| ActionError::Child(SetupDefaultProfile::action_tag(), Box::new(e)))?;

        let configure_shell_profile = if settings.modify_profile {
            Some(ConfigureShellProfile::plan().await.map_err(|e| {
                ActionError::Child(ConfigureShellProfile::action_tag(), Box::new(e))
            })?)
        } else {
            None
        };
        let place_channel_configuration =
            PlaceChannelConfiguration::plan(settings.channels.clone(), settings.force)
                .await
                .map_err(|e| {
                    ActionError::Child(PlaceChannelConfiguration::action_tag(), Box::new(e))
                })?;
        let place_nix_configuration = PlaceNixConfiguration::plan(
            settings.nix_build_group_name.clone(),
            settings.extra_conf.clone(),
            settings.force,
        )
        .await
        .map_err(|e| ActionError::Child(PlaceNixConfiguration::action_tag(), Box::new(e)))?;

        Ok(Self {
            place_channel_configuration,
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
            place_channel_configuration,
            configure_shell_profile,
        } = &self;

        let mut buf = setup_default_profile.describe_execute();
        buf.append(&mut place_nix_configuration.describe_execute());
        buf.append(&mut place_channel_configuration.describe_execute());
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
            place_channel_configuration,
            configure_shell_profile,
        } = self;

        if let Some(configure_shell_profile) = configure_shell_profile {
            let setup_default_profile_span = tracing::Span::current().clone();
            let (
                place_nix_configuration_span,
                place_channel_configuration_span,
                configure_shell_profile_span,
            ) = (
                setup_default_profile_span.clone(),
                setup_default_profile_span.clone(),
                setup_default_profile_span.clone(),
            );
            tokio::try_join!(
                async move {
                    setup_default_profile
                        .try_execute()
                        .instrument(setup_default_profile_span)
                        .await
                        .map_err(|e| {
                            ActionError::Child(setup_default_profile.action_tag(), Box::new(e))
                        })
                },
                async move {
                    place_nix_configuration
                        .try_execute()
                        .instrument(place_nix_configuration_span)
                        .await
                        .map_err(|e| {
                            ActionError::Child(place_nix_configuration.action_tag(), Box::new(e))
                        })
                },
                async move {
                    place_channel_configuration
                        .try_execute()
                        .instrument(place_channel_configuration_span)
                        .await
                        .map_err(|e| {
                            ActionError::Child(
                                place_channel_configuration.action_tag(),
                                Box::new(e),
                            )
                        })
                },
                async move {
                    configure_shell_profile
                        .try_execute()
                        .instrument(configure_shell_profile_span)
                        .await
                        .map_err(|e| {
                            ActionError::Child(configure_shell_profile.action_tag(), Box::new(e))
                        })
                },
            )?;
        } else {
            let place_channel_configuration_span = tracing::Span::current().clone();
            let (setup_default_profile_span, place_nix_configuration_span) = (
                place_channel_configuration_span.clone(),
                place_channel_configuration_span.clone(),
            );
            tokio::try_join!(
                async move {
                    setup_default_profile
                        .try_execute()
                        .instrument(setup_default_profile_span)
                        .await
                        .map_err(|e| {
                            ActionError::Child(setup_default_profile.action_tag(), Box::new(e))
                        })
                },
                async move {
                    place_nix_configuration
                        .try_execute()
                        .instrument(place_nix_configuration_span)
                        .await
                        .map_err(|e| {
                            ActionError::Child(place_nix_configuration.action_tag(), Box::new(e))
                        })
                },
                async move {
                    place_channel_configuration
                        .try_execute()
                        .instrument(place_channel_configuration_span)
                        .await
                        .map_err(|e| {
                            ActionError::Child(
                                place_channel_configuration.action_tag(),
                                Box::new(e),
                            )
                        })
                },
            )?;
        };

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            setup_default_profile,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
        } = &self;

        let mut buf = Vec::default();
        if let Some(configure_shell_profile) = configure_shell_profile {
            buf.append(&mut configure_shell_profile.describe_revert());
        }
        buf.append(&mut place_channel_configuration.describe_revert());
        buf.append(&mut place_nix_configuration.describe_revert());
        buf.append(&mut setup_default_profile.describe_revert());

        buf
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        if let Some(configure_shell_profile) = &mut self.configure_shell_profile {
            configure_shell_profile.try_revert().await.map_err(|e| {
                ActionError::Child(configure_shell_profile.action_tag(), Box::new(e))
            })?;
        }
        self.place_channel_configuration
            .try_revert()
            .await
            .map_err(|e| {
                ActionError::Child(self.place_channel_configuration.action_tag(), Box::new(e))
            })?;
        self.place_nix_configuration
            .try_revert()
            .await
            .map_err(|e| {
                ActionError::Child(self.place_nix_configuration.action_tag(), Box::new(e))
            })?;
        self.setup_default_profile.try_revert().await.map_err(|e| {
            ActionError::Child(self.setup_default_profile.action_tag(), Box::new(e))
        })?;

        Ok(())
    }
}
