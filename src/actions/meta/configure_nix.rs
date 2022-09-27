use serde::Serialize;

use crate::actions::{
    base::{
        ConfigureNixDaemonService, ConfigureNixDaemonServiceError,
        SetupDefaultProfile, SetupDefaultProfileError,
    },
    meta::{
        ConfigureShellProfile, ConfigureShellProfileError,
        PlaceChannelConfiguration, PlaceChannelConfigurationError,
        PlaceNixConfiguration, PlaceNixConfigurationError,
    },
    Action, ActionState,
};
use crate::InstallSettings;

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
        let place_nix_configuration = PlaceNixConfiguration::plan(
            settings.nix_build_group_name,
            settings.extra_conf,
            settings.force,
        )
        .await?;
        let configure_nix_daemon_service = ConfigureNixDaemonService::plan().await?;

        Ok(Self {
            place_channel_configuration,
            place_nix_configuration,
            setup_default_profile,
            configure_nix_daemon_service,
            configure_shell_profile,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ConfigureNix {
    type Error = ConfigureNixError;
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
            action_state: _,
        } = &self;

        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            let mut buf = setup_default_profile.describe_execute();
            buf.append(&mut configure_nix_daemon_service.describe_execute());
            buf.append(&mut place_nix_configuration.describe_execute());
            buf.append(&mut place_channel_configuration.describe_execute());
            if let Some(configure_shell_profile) = configure_shell_profile {
                buf.append(&mut configure_shell_profile.describe_execute());
            }
            buf
        }
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
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Configuring nix");
            return Ok(());
        }
        tracing::debug!("Configuring nix");

        if let Some(configure_shell_profile) = configure_shell_profile {
            tokio::try_join!(
                async move {
                    setup_default_profile
                        .execute()
                        .await
                        .map_err(|e| ConfigureNixError::from(e))
                },
                async move {
                    place_nix_configuration
                        .execute()
                        .await
                        .map_err(|e| ConfigureNixError::from(e))
                },
                async move {
                    place_channel_configuration
                        .execute()
                        .await
                        .map_err(|e| ConfigureNixError::from(e))
                },
                async move {
                    configure_shell_profile
                        .execute()
                        .await
                        .map_err(|e| ConfigureNixError::from(e))
                },
            )?;
        } else {
            tokio::try_join!(
                async move {
                    setup_default_profile
                        .execute()
                        .await
                        .map_err(|e| ConfigureNixError::from(e))
                },
                async move {
                    place_nix_configuration
                        .execute()
                        .await
                        .map_err(|e| ConfigureNixError::from(e))
                },
                async move {
                    place_channel_configuration
                        .execute()
                        .await
                        .map_err(|e| ConfigureNixError::from(e))
                },
            )?;
        };
        configure_nix_daemon_service.execute().await?;

        tracing::trace!("Configured nix");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        let Self {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
            action_state: _,
        } = &self;

        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            let mut buf = Vec::default();
            if let Some(configure_shell_profile) = configure_shell_profile {
                buf.append(&mut configure_shell_profile.describe_revert());
            }
            buf.append(&mut place_channel_configuration.describe_revert());
            buf.append(&mut place_nix_configuration.describe_revert());
            buf.append(&mut configure_nix_daemon_service.describe_revert());
            buf.append(&mut setup_default_profile.describe_revert());

            buf
        }
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
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Unconfiguring nix");
            return Ok(());
        }
        tracing::debug!("Unconfiguring nix");

        configure_nix_daemon_service.revert().await?;
        if let Some(configure_shell_profile) = configure_shell_profile {
            configure_shell_profile.revert().await?;
        }
        place_channel_configuration.revert().await?;
        place_nix_configuration.revert().await?;
        setup_default_profile.revert().await?;

        tracing::trace!("Unconfigured nix");
        *action_state = ActionState::Uncompleted;
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
    #[error("Setting up default profile")]
    SetupDefaultProfile(#[source] #[from] SetupDefaultProfileError),
    #[error("Placing Nix configuration")]
    PlaceNixConfiguration(#[source] #[from] PlaceNixConfigurationError),
    #[error("Placing channel configuration")]
    PlaceChannelConfiguration(#[source] #[from] PlaceChannelConfigurationError),
    #[error("Configuring Nix daemon")]
    ConfigureNixDaemonService(#[source] #[from] ConfigureNixDaemonServiceError),
    #[error("Configuring shell profile")]
    ConfigureShellProfile(#[source] #[from] ConfigureShellProfileError),
}
