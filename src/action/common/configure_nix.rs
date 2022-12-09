use crate::{
    action::{
        base::SetupDefaultProfile,
        common::{ConfigureShellProfile, PlaceChannelConfiguration, PlaceNixConfiguration},
        linux::ConfigureNixDaemonService,
        Action, ActionDescription, ActionError, StatefulAction,
    },
    channel_value::ChannelValue,
    settings::CommonSettings,
};

use reqwest::Url;

/**
Configure Nix and start it
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNix {
    setup_default_profile: StatefulAction<SetupDefaultProfile>,
    configure_shell_profile: Option<StatefulAction<ConfigureShellProfile>>,
    place_channel_configuration: StatefulAction<PlaceChannelConfiguration>,
    place_nix_configuration: StatefulAction<PlaceNixConfiguration>,
    configure_nix_daemon_service: StatefulAction<ConfigureNixDaemonService>,
}

impl ConfigureNix {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(settings: &CommonSettings) -> Result<StatefulAction<Self>, ActionError> {
        let channels: Vec<(String, Url)> = settings
            .channels
            .iter()
            .map(|ChannelValue(channel, url)| (channel.to_string(), url.clone()))
            .collect();

        let setup_default_profile =
            SetupDefaultProfile::plan(channels.iter().map(|(v, _k)| v.clone()).collect()).await?;

        let configure_shell_profile = if settings.modify_profile {
            Some(ConfigureShellProfile::plan().await?)
        } else {
            None
        };
        let place_channel_configuration =
            PlaceChannelConfiguration::plan(channels, settings.force).await?;
        let place_nix_configuration = PlaceNixConfiguration::plan(
            settings.nix_build_group_name.clone(),
            settings.extra_conf.clone(),
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
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "configure_nix")]
impl Action for ConfigureNix {
    fn tracing_synopsis(&self) -> String {
        "Configure Nix".to_string()
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let Self {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
        } = &self;

        let mut buf = setup_default_profile.describe_execute();
        buf.append(&mut configure_nix_daemon_service.describe_execute());
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
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
        } = self;

        if let Some(configure_shell_profile) = configure_shell_profile {
            tokio::try_join!(
                async move { setup_default_profile.try_execute().await },
                async move { place_nix_configuration.try_execute().await },
                async move { place_channel_configuration.try_execute().await },
                async move { configure_shell_profile.try_execute().await },
            )?;
        } else {
            tokio::try_join!(
                async move { setup_default_profile.try_execute().await },
                async move { place_nix_configuration.try_execute().await },
                async move { place_channel_configuration.try_execute().await },
            )?;
        };
        configure_nix_daemon_service.try_execute().await?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            setup_default_profile,
            configure_nix_daemon_service,
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
        buf.append(&mut configure_nix_daemon_service.describe_revert());
        buf.append(&mut setup_default_profile.describe_revert());

        buf
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            place_channel_configuration,
            configure_shell_profile,
        } = self;

        configure_nix_daemon_service.try_revert().await?;
        if let Some(configure_shell_profile) = configure_shell_profile {
            configure_shell_profile.try_revert().await?;
        }
        place_channel_configuration.try_revert().await?;
        place_nix_configuration.try_revert().await?;
        setup_default_profile.try_revert().await?;

        Ok(())
    }
}
