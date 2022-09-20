use tokio::task::JoinSet;

use crate::actions::base::{SetupDefaultProfile, ConfigureNixDaemonService, ConfigureShellProfile, SetupDefaultProfileReceipt, ConfigureNixDaemonServiceReceipt, ConfigureShellProfileReceipt};
use crate::{HarmonicError, InstallSettings};

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNix {
    setup_default_profile: SetupDefaultProfile,
    configure_nix_daemon_service: ConfigureNixDaemonService,
    configure_shell_profile: Option<ConfigureShellProfile>,
}

impl ConfigureNix {
    pub fn plan(settings: InstallSettings) -> Self {
        let setup_default_profile = SetupDefaultProfile::plan();
        let configure_nix_daemon_service = ConfigureNixDaemonService::plan();

        let configure_shell_profile = if settings.modify_profile {
            Some(ConfigureShellProfile::plan())
        } else {
            None
        };
        

        Self { setup_default_profile, configure_nix_daemon_service, configure_shell_profile }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for ConfigureNix {
    type Receipt = ConfigureNixReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { setup_default_profile, configure_nix_daemon_service, configure_shell_profile } = &self;

        let mut buf = setup_default_profile.description();
        buf.append(&mut configure_nix_daemon_service.description());
        if let Some(configure_shell_profile) = configure_shell_profile {
            buf.append(&mut configure_shell_profile.description());
        }
        
        buf
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { setup_default_profile, configure_nix_daemon_service, configure_shell_profile } = self;

        let (setup_default_profile, configure_nix_daemon_service, configure_shell_profile) = if let Some(configure_shell_profile) = configure_shell_profile {
            let (a, b, c) = tokio::try_join!(
                setup_default_profile.execute(),
                configure_nix_daemon_service.execute(),
                configure_shell_profile.execute(),
            )?;
            (a, b, Some(c))
        } else {
            let (a, b) = tokio::try_join!(
                setup_default_profile.execute(),
                configure_nix_daemon_service.execute(),
            )?;
            (a, b, None)
        };

        Ok(Self::Receipt {
            setup_default_profile,
            configure_nix_daemon_service,
            configure_shell_profile,
        })

    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixReceipt {
    setup_default_profile: SetupDefaultProfileReceipt,
    configure_nix_daemon_service: ConfigureNixDaemonServiceReceipt,
    configure_shell_profile: Option<ConfigureShellProfileReceipt>,
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
