use serde::Serialize;

use crate::actions::{
    base::{
        ConfigureNixDaemonService, ConfigureNixDaemonServiceError, PlaceNixConfiguration,
        PlaceNixConfigurationError, SetupDefaultProfile, SetupDefaultProfileError, PlaceChannelConfiguration, PlaceChannelConfigurationError,
    },
    meta::{ConfigureShellProfile, ConfigureShellProfileError}, ActionState, Action, ActionError,
};
use crate::{HarmonicError, InstallSettings};

use crate::actions::{ActionDescription, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNix {
    setup_default_profile: SetupDefaultProfile,
    configure_shell_profile: Option<ConfigureShellProfile>,
    place_channel_configuration: PlaceChannelConfiguration,
    place_nix_configuration: PlaceNixConfiguration,
    configure_nix_daemon_service: ConfigureNixDaemonService,
    action_state: ActionState,
}

impl ConfigureNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(settings: InstallSettings) -> Result<Self, ConfigureNixError> {
        let channels = settings
            .channels
            .iter()
            .map(|(channel, _)| channel.to_string())
            .collect();

        let setup_default_profile = SetupDefaultProfile::plan(channels).await?;

        let configure_shell_profile = if settings.modify_profile {
            Some(ConfigureShellProfile::plan().await?)
        } else {
            None
        };
        let place_channel_configuration =
            PlaceChannelConfiguration::plan(settings.channels, settings.force).await?;
        let place_nix_configuration =
            PlaceNixConfiguration::plan(settings.nix_build_group_name, settings.extra_conf, settings.force).await?;
        let configure_nix_daemon_service = ConfigureNixDaemonService::plan().await?;

        Ok(Self {
            place_channel_configuration,
            place_nix_configuration,
            setup_default_profile,
            configure_nix_daemon_service,
            configure_shell_profile,
            action_state: ActionState::Planned,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ConfigureNix {
    type Error = ConfigureNixError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
            action_state: _,
        } = &self;

        let mut buf = setup_default_profile.description();
        buf.append(&mut configure_nix_daemon_service.description());
        buf.append(&mut place_nix_configuration.description());
        buf.append(&mut place_channel_configuration.description());
        if let Some(configure_shell_profile) = configure_shell_profile {
            buf.append(&mut configure_shell_profile.description());
        }

        buf
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
            action_state,
        } = self;

        if let Some(configure_shell_profile) = configure_shell_profile {
            tokio::try_join!(
                async move { setup_default_profile.execute().await.map_err(|e| ConfigureNixError::from(e)) },
                async move { place_nix_configuration.execute().await.map_err(|e| ConfigureNixError::from(e)) },
                async move { place_channel_configuration.execute().await.map_err(|e| ConfigureNixError::from(e)) },
                async move { configure_shell_profile.execute().await.map_err(|e| ConfigureNixError::from(e)) },
            )?;
        } else {
            tokio::try_join!(
                async move { setup_default_profile.execute().await.map_err(|e| ConfigureNixError::from(e)) },
                async move { place_nix_configuration.execute().await.map_err(|e| ConfigureNixError::from(e)) },
                async move { place_channel_configuration.execute().await.map_err(|e| ConfigureNixError::from(e)) },
            )?;
        };
        configure_nix_daemon_service.execute().await?;

        *action_state = ActionState::Completed;
        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
            action_state,
        } = self;

        configure_nix_daemon_service.revert().await?;
        if let Some(configure_shell_profile) = configure_shell_profile {
            configure_shell_profile.revert().await?;
        }
        place_channel_configuration.revert().await?;
        place_nix_configuration.revert().await?;
        setup_default_profile.revert().await?;

        *action_state = ActionState::Reverted;
        Ok(())
    }
}

impl From<ConfigureNix> for Action {
    fn from(v: ConfigureNix) -> Self {
        Action::ConfigureNix(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum ConfigureNixError {
    #[error(transparent)]
    SetupDefaultProfile(#[from] SetupDefaultProfileError),
    #[error(transparent)]
    PlaceNixConfiguration(#[from] PlaceNixConfigurationError),
    #[error(transparent)]
    PlaceChannelConfiguration(#[from] PlaceChannelConfigurationError),
    #[error(transparent)]
    ConfigureNixDaemonService(#[from] ConfigureNixDaemonServiceError),
    #[error(transparent)]
    ConfigureShellProfile(#[from] ConfigureShellProfileError),
}