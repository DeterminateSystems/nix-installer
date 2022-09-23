use serde::Serialize;

use crate::actions::{
    base::{
        ConfigureNixDaemonService, ConfigureNixDaemonServiceError, PlaceNixConfiguration,
        PlaceNixConfigurationError, SetupDefaultProfile, SetupDefaultProfileError, PlaceChannelConfiguration, PlaceChannelConfigurationError,
    },
    meta::{ConfigureShellProfile, ConfigureShellProfileError}, ActionState, Action,
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
}

impl ConfigureNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(settings: InstallSettings) -> Result<ActionState<Self>, ConfigureNixError> {
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
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<ConfigureNix> {
    type Error = ConfigureNixError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
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
        } = self;

        let (
            setup_default_profile,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
        ) = if let Some(configure_shell_profile) = configure_shell_profile {
            let (a, b, c, d) = tokio::try_join!(
                setup_default_profile.execute(),
                place_nix_configuration.execute(),
                place_channel_configuration.execute(),
                configure_shell_profile.execute(),
            )?;
            (a, b, c, Some(d))
        } else {
            let (a, b, c) = tokio::try_join!(
                setup_default_profile.execute(),
                place_nix_configuration.execute(),
                place_channel_configuration.execute(),
            )?;
            (a, b, c, None)
        };
        let configure_nix_daemon_service = configure_nix_daemon_service.execute().await?;

        Ok(Self::Receipt {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
        })
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<ConfigureNix>> for ActionState<Action> {
    fn from(v: ActionState<ConfigureNix>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::ConfigureNix(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::ConfigureNix(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::ConfigureNix(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum ConfigureNixError {

}