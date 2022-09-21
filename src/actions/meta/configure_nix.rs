use tokio::task::JoinSet;

use crate::actions::base::{SetupDefaultProfile, ConfigureNixDaemonService, ConfigureShellProfile, SetupDefaultProfileReceipt, ConfigureNixDaemonServiceReceipt, ConfigureShellProfileReceipt, PlaceNixConfigurationReceipt, PlaceNixConfiguration};
use crate::{HarmonicError, InstallSettings, Harmonic};

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNix {
    setup_default_profile: SetupDefaultProfile,
    configure_shell_profile: Option<ConfigureShellProfile>,
    place_nix_configuration: PlaceNixConfiguration,
    configure_nix_daemon_service: ConfigureNixDaemonService,
}

impl ConfigureNix {
    pub async fn plan(settings: InstallSettings) -> Result<Self, HarmonicError> {
        let channels = settings.channels.iter().map(|(channel, _)| channel.to_string()).collect();
        
        let setup_default_profile = SetupDefaultProfile::plan(channels).await?;

        let configure_shell_profile = if settings.modify_profile {
            Some(ConfigureShellProfile::plan().await?)
        } else {
            None
        };
        let place_nix_configuration = PlaceNixConfiguration::plan(settings.nix_build_group_name, settings.extra_conf).await?;
        let configure_nix_daemon_service = ConfigureNixDaemonService::plan().await?;
        

        Ok(Self { place_nix_configuration, setup_default_profile, configure_nix_daemon_service, configure_shell_profile })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for ConfigureNix {
    type Receipt = ConfigureNixReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { setup_default_profile, configure_nix_daemon_service, place_nix_configuration, configure_shell_profile } = &self;

        let mut buf = setup_default_profile.description();
        buf.append(&mut configure_nix_daemon_service.description());
        buf.append(&mut place_nix_configuration.description());
        if let Some(configure_shell_profile) = configure_shell_profile {
            buf.append(&mut configure_shell_profile.description());
        }
        
        buf
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { setup_default_profile, configure_nix_daemon_service, place_nix_configuration, configure_shell_profile } = self;

        let (setup_default_profile, configure_nix_daemon_service, place_nix_configuration, configure_shell_profile) = if let Some(configure_shell_profile) = configure_shell_profile {
            let (a, b, c, d) = tokio::try_join!(
                setup_default_profile.execute(),
                configure_nix_daemon_service.execute(),
                place_nix_configuration.execute(),
                configure_shell_profile.execute(),
            )?;
            (a, b, c, Some(d))
        } else {
            let (a, b, c) = tokio::try_join!(
                setup_default_profile.execute(),
                configure_nix_daemon_service.execute(),
                place_nix_configuration.execute(),
            )?;
            (a, b, c, None)
        };

        Ok(Self::Receipt {
            setup_default_profile,
            configure_nix_daemon_service,
            place_nix_configuration,
            configure_shell_profile,
        })

    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixReceipt {
    setup_default_profile: SetupDefaultProfileReceipt,
    configure_shell_profile: Option<ConfigureShellProfileReceipt>,
    place_nix_configuration: PlaceNixConfigurationReceipt,
    configure_nix_daemon_service: ConfigureNixDaemonServiceReceipt,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for ConfigureNixReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Stop the systemd Nix daemon".to_string(),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            ),
        ]
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
