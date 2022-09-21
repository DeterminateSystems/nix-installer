use serde::{Deserialize, Serialize};

use crate::{
    actions::{
        meta::{ConfigureNix, ProvisionNix, StartNixDaemon},
        Action, ActionDescription, ActionReceipt, Actionable, Revertable,
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
    actions: Vec<Action>,
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
            actions = self
                .actions
                .iter()
                .flat_map(|action| action.description())
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
                .join("\n"),
        )
    }
    pub async fn new(settings: InstallSettings) -> Result<Self, HarmonicError> {
        let actions = vec![
            Action::ProvisionNix(ProvisionNix::plan(settings.clone()).await?),
            Action::ConfigureNix(ConfigureNix::plan(settings.clone()).await?),
            Action::StartNixDaemon(StartNixDaemon::plan().await?),
        ];
        Ok(Self { settings, actions })
    }

    #[tracing::instrument(skip_all)]
    pub async fn install(self) -> Result<Receipt, HarmonicError> {
        let mut receipt = Receipt::default();
        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.
        for action in self.actions {
            match action.execute().await {
                Ok(action_receipt) => receipt.actions.push(action_receipt),
                Err(err) => {
                    return Err(err);
                    // TODO 
                    // let mut revert_errs = Vec::default();

                    // for action_receipt in receipt.actions {
                    //     if let Err(err) = action_receipt.revert().await {
                    //         revert_errs.push(err);
                    //     }
                    // }
                    // if !revert_errs.is_empty() {
                    //     return Err(HarmonicError::FailedReverts(vec![err], revert_errs));
                    // }
                },
            };
        }
        Ok(receipt)
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Receipt {
    actions: Vec<ActionReceipt>,
}
