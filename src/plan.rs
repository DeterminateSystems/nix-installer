use serde::{Deserialize, Serialize};

use crate::{
    actions::{
        meta::{ConfigureNix, ProvisionNix, StartNixDaemon, ProvisionNixReceipt, ConfigureNixReceipt, StartNixDaemonReceipt},
        Action, ActionDescription, ActionReceipt, Actionable, Revertable,
    },
    settings::InstallSettings,
    HarmonicError, error::ActionState,
};

#[derive(thiserror::Error, Debug)]
struct InstallError {

}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}


#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct InstallPlan {
    settings: InstallSettings,

    provision_nix: ProvisionNix,
    configure_nix: ConfigureNix,
    start_nix_daemon: StartNixDaemon,
}

impl InstallPlan {
    #[tracing::instrument(skip_all)]
    pub fn description(&self) -> String {
        format!(
            "\
            This Nix install is for:\n\
              Operating System: {os_type}\n\
              Init system: {init_type}\n\
              Nix channels: {nix_channels}\n\
            \n\
            The following actions will be taken:\n\
            {actions}
        ",
            os_type = "Linux",
            init_type = "systemd",
            nix_channels = self
                .settings
                .channels
                .iter()
                .map(|(name, url)| format!("{name}={url}"))
                .collect::<Vec<_>>()
                .join(","),
            
            actions = {
                let mut buf = self.provision_nix.description();
                buf.append(&mut self.configure_nix.description());
                buf.append(&mut self.start_nix_daemon.description());

                buf
                    .iter()
                    .map(|desc| {
                        let ActionDescription {
                            description,
                            explanation,
                        } = desc;

                        let mut buf = String::default();
                        buf.push_str(&format!("* {description}\n"));
                        if self.settings.explain {
                            for line in explanation {
                                buf.push_str(&format!("  {line}\n"));
                            }
                        }
                        buf
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        )
    }
    pub async fn new(settings: InstallSettings) -> Result<Self, HarmonicError> {
        let settings_clone_1 = settings.clone();
        let settings_clone_2 = settings.clone();
        Ok(Self {
            settings,
            provision_nix: ProvisionNix::plan(settings_clone_1).await?,
            configure_nix: ConfigureNix::plan(settings_clone_2).await?,
            start_nix_daemon: StartNixDaemon::plan().await?,
        })
    }

    #[tracing::instrument(skip_all)]
    pub async fn install(self) -> Result<InstallReceipt, InstallReceipt> {
        let Self { settings: _, provision_nix, configure_nix, start_nix_daemon } = self;
        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.

        let mut errored = false;
        let receipt = InstallReceipt {
            provision_nix: match provision_nix.execute().await {
                Ok(success) => success,
                Err(err) => {
                    errored = true;
                    err
                },
            },
            configure_nix: match configure_nix.execute().await {
                Ok(success) => success,
                Err(err) => {
                    errored = true;
                    err
                },
            },
            start_nix_daemon: match start_nix_daemon.execute().await {
                Ok(success) => success,
                Err(err) => {
                    errored = true;
                    err
                },
            },
        };
        match errored {
            true => Err(receipt),
            false => Ok(receipt),
        }
    }
}

#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
pub struct InstallReceipt {
    provision_nix: ActionState<ProvisionNix>,
    configure_nix: ActionState<ConfigureNix>,
    start_nix_daemon: ActionState<StartNixDaemon>,
}

impl<'a> std::fmt::Display for InstallReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}