use serde::{Deserialize, Serialize};

use crate::{settings::InstallSettings, actions::{Action, StartNixDaemonService, Actionable, ActionReceipt, Revertable}, HarmonicError};



#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
struct InstallPlan {
    settings: InstallSettings,

    /** Bootstrap the install

    * There are roughly three phases:
    * download_nix  --------------------------------------> move_downloaded_nix
    * create_group -> create_users -> create_directories -> move_downloaded_nix
    * place_channel_configuration
    * place_nix_configuration
    * ---
    * setup_default_profile
    * configure_nix_daemon_service
    * configure_shell_profile
    * ---
    * start_nix_daemon_service
    */
    actions: Vec<Action>,
}

impl InstallPlan {
    async fn plan(settings: InstallSettings) -> Result<Self, HarmonicError> {
        let start_nix_daemon_service = StartNixDaemonService::plan();

        let actions = vec![
            Action::StartNixDaemonService(start_nix_daemon_service),
        ];
        Ok(Self { settings, actions })
    }
    async fn install(self) -> Result<Receipt, HarmonicError> {
        let mut receipt = Receipt::default();
        // This is **deliberately sequential**.
        // Actions which are parallelizable are represented by "group actions" like CreateUsers
        // The plan itself represents the concept of the sequence of stages.
        for action in self.actions {
            match action.execute().await {
                Ok(action_receipt) => receipt.actions.push(action_receipt),
                Err(err) => {
                    let mut revert_errs = Vec::default();

                    for action_receipt in receipt.actions {
                        if let Err(err) = action_receipt.revert().await {
                            revert_errs.push(err);
                        }
                    }
                    if !revert_errs.is_empty() {
                        return Err(HarmonicError::FailedReverts(vec![err], revert_errs))
                    }

                    return Err(err)

                },
            };
        }
       Ok(receipt)
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct Receipt {
    actions: Vec<ActionReceipt>,
}
