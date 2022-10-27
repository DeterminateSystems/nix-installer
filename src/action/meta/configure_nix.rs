use reqwest::Url;

use crate::{
    action::{
        base::{ConfigureNixDaemonService, SetupDefaultProfile},
        meta::{ConfigureShellProfile, PlaceChannelConfiguration, PlaceNixConfiguration},
        Action, ActionDescription, ActionState,
    },
    cli::arg::ChannelValue,
    BoxableError, CommonSettings,
};

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
    pub async fn plan(
        settings: CommonSettings,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let channels: Vec<(String, Url)> = settings
            .channels
            .iter()
            .map(|ChannelValue(channel, url)| (channel.to_string(), url.clone()))
            .collect();

        let setup_default_profile =
            SetupDefaultProfile::plan(channels.iter().map(|(v, _k)| v.clone()).collect())
                .await
                .map_err(|e| e.boxed())?;

        let configure_shell_profile = if settings.modify_profile {
            Some(ConfigureShellProfile::plan().await?)
        } else {
            None
        };
        let place_channel_configuration =
            PlaceChannelConfiguration::plan(channels, settings.force).await?;
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
#[typetag::serde(name = "configure-nix")]
impl Action for ConfigureNix {
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
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        *action_state = ActionState::Progress;
        tracing::debug!("Configuring nix");

        if let Some(configure_shell_profile) = configure_shell_profile {
            tokio::try_join!(
                async move { setup_default_profile.execute().await },
                async move { place_nix_configuration.execute().await },
                async move { place_channel_configuration.execute().await },
                async move { configure_shell_profile.execute().await },
            )?;
        } else {
            tokio::try_join!(
                async move { setup_default_profile.execute().await },
                async move { place_nix_configuration.execute().await },
                async move { place_channel_configuration.execute().await },
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
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        *action_state = ActionState::Progress;
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
