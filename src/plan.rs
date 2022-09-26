use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::fs::File;

use crate::{
    actions::{
        meta::{ConfigureNix, ProvisionNix, StartNixDaemon},
        Action, ActionDescription, Actionable, ActionState, ActionError,
    },
    settings::InstallSettings,
    HarmonicError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct InstallPlan {
    settings: InstallSettings,

    /** Bootstrap the install

    * There are roughly three phases:
    * "Create nix tree"":
    * download_nix  --------------------------------------> move_downloaded_nix
    * create_group -> create_users -> create_directories -> move_downloaded_nix
    * place_channel_configuration
    * place_nix_configuration
    * ---
    * "Configure Nix":
    * setup_default_profile
    * configure_nix_daemon_service
    * configure_shell_profile
    * ---
    * "Start Nix"
    * start_nix_daemon_service
    */
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
                buf.iter()
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
        Ok(Self {
            settings: settings.clone(),
            provision_nix: ProvisionNix::plan(settings.clone()).await
                .map_err(|e| ActionError::from(e))?,
            configure_nix: ConfigureNix::plan(settings).await
                .map_err(|e| ActionError::from(e))?,
            start_nix_daemon: StartNixDaemon::plan().await
                .map_err(|e| ActionError::from(e))?,
        })
    }

    #[tracing::instrument(skip_all)]
    pub async fn install(&mut self) -> Result<(), HarmonicError> {
        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.
        self.provision_nix.execute().await
            .map_err(|e| ActionError::from(e))?;
        self.configure_nix.execute().await
            .map_err(|e| ActionError::from(e))?;
        self.start_nix_daemon.execute().await
            .map_err(|e| ActionError::from(e))?;

        let install_receipt_path = PathBuf::from("/nix/receipt.json");
        let self_json = serde_json::to_string_pretty(&self)
            .map_err(HarmonicError::SerializingReceipt)?;
        tokio::fs::write(&install_receipt_path, self_json).await
            .map_err(|e| HarmonicError::RecordingReceipt(install_receipt_path, e))?;

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn revert(&mut self) -> Result<(), HarmonicError> {
        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.
        self.start_nix_daemon.revert().await
            .map_err(|e| ActionError::from(e))?;
        self.configure_nix.revert().await
            .map_err(|e| ActionError::from(e))?;
        self.provision_nix.revert().await
            .map_err(|e| ActionError::from(e))?;

        Ok(())
    }
}
