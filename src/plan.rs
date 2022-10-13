use std::path::PathBuf;

use crate::{
    actions::{
        meta::{ConfigureNix, ProvisionNix, StartNixDaemon},
        ActionDescription, ActionError, Actionable,
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
    pub async fn new(settings: InstallSettings) -> Result<Self, HarmonicError> {
        Ok(Self {
            settings: settings.clone(),
            provision_nix: ProvisionNix::plan(settings.clone())
                .await
                .map_err(|e| ActionError::from(e))?,
            configure_nix: ConfigureNix::plan(settings)
                .await
                .map_err(|e| ActionError::from(e))?,
            start_nix_daemon: StartNixDaemon::plan()
                .await
                .map_err(|e| ActionError::from(e))?,
        })
    }

    #[tracing::instrument(skip_all)]
    pub fn describe_execute(&self, explain: bool) -> String {
        let Self {
            settings,
            provision_nix,
            configure_nix,
            start_nix_daemon,
        } = self;
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
            nix_channels = settings
                .channels
                .iter()
                .map(|(name, url)| format!("{name}={url}"))
                .collect::<Vec<_>>()
                .join(","),
            actions = {
                let mut buf = provision_nix.describe_execute();
                buf.append(&mut configure_nix.describe_execute());
                buf.append(&mut start_nix_daemon.describe_execute());
                buf.iter()
                    .map(|desc| {
                        let ActionDescription {
                            description,
                            explanation,
                        } = desc;

                        let mut buf = String::default();
                        buf.push_str(&format!("* {description}\n"));
                        if explain {
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

    #[tracing::instrument(skip_all)]
    pub async fn install(&mut self) -> Result<(), HarmonicError> {
        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.

        if let Err(err) = self.provision_nix.execute().await {
            write_receipt(self.clone()).await?;
            return Err(ActionError::from(err).into());
        }

        if let Err(err) = self.configure_nix.execute().await {
            write_receipt(self.clone()).await?;
            return Err(ActionError::from(err).into());
        }

        if let Err(err) = self.start_nix_daemon.execute().await {
            write_receipt(self.clone()).await?;
            return Err(ActionError::from(err).into());
        }

        write_receipt(self.clone()).await?;

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub fn describe_revert(&self, explain: bool) -> String {
        let Self {
            settings,
            provision_nix,
            configure_nix,
            start_nix_daemon,
        } = self;
        format!(
            "\
            This Nix uninstall is for:\n\
              Operating System: {os_type}\n\
              Init system: {init_type}\n\
              Nix channels: {nix_channels}\n\
            \n\
            The following actions will be taken:\n\
            {actions}
        ",
            os_type = "Linux",
            init_type = "systemd",
            nix_channels = settings
                .channels
                .iter()
                .map(|(name, url)| format!("{name}={url}"))
                .collect::<Vec<_>>()
                .join(","),
            actions = {
                let mut buf = provision_nix.describe_revert();
                buf.append(&mut configure_nix.describe_revert());
                buf.append(&mut start_nix_daemon.describe_revert());
                buf.iter()
                    .map(|desc| {
                        let ActionDescription {
                            description,
                            explanation,
                        } = desc;

                        let mut buf = String::default();
                        buf.push_str(&format!("* {description}\n"));
                        if explain {
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

    #[tracing::instrument(skip_all)]
    pub async fn revert(&mut self) -> Result<(), HarmonicError> {
        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.
        if let Err(err) = self.start_nix_daemon.revert().await {
            write_receipt(self.clone()).await?;
            return Err(ActionError::from(err).into());
        }

        if let Err(err) = self.configure_nix.revert().await {
            write_receipt(self.clone()).await?;
            return Err(ActionError::from(err).into());
        }

        if let Err(err) = self.provision_nix.revert().await {
            write_receipt(self.clone()).await?;
            return Err(ActionError::from(err).into());
        }

        Ok(())
    }
}

async fn write_receipt(plan: InstallPlan) -> Result<(), HarmonicError> {
    tokio::fs::create_dir_all("/nix")
        .await
        .map_err(|e| HarmonicError::RecordingReceipt(PathBuf::from("/nix"), e))?;
    let install_receipt_path = PathBuf::from("/nix/receipt.json");
    let self_json =
        serde_json::to_string_pretty(&plan).map_err(HarmonicError::SerializingReceipt)?;
    tokio::fs::write(&install_receipt_path, self_json)
        .await
        .map_err(|e| HarmonicError::RecordingReceipt(install_receipt_path, e))?;
    Result::<(), HarmonicError>::Ok(())
}
