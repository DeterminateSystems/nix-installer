use std::io::Cursor;

use tokio::process::Command;

use crate::{
    actions::{
        base::darwin::KickstartLaunchctlService,
        meta::{darwin::CreateApfsVolume, ConfigureNix, ProvisionNix},
        Action, ActionError,
    },
    execute_command,
    os::darwin::DiskUtilOutput,
    planner::{Plannable, PlannerError},
    BuiltinPlanner, InstallPlan, InstallSettings,
};

#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct DarwinMulti {
    #[clap(flatten)]
    settings: InstallSettings,
}

#[async_trait::async_trait]
impl Plannable for DarwinMulti {
    const DISPLAY_STRING: &'static str = "Darwin Multi-User";
    const SLUG: &'static str = "darwin-multi";

    fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: InstallSettings::default()?,
        })
    }

    async fn plan(self) -> Result<crate::InstallPlan, crate::planner::PlannerError> {
        let root_disk = {
            let buf =
                execute_command(Command::new("/usr/sbin/diskutil").args(["info", "-plist", "/"]))
                    .await
                    .unwrap()
                    .stdout;
            let the_plist: DiskUtilOutput = plist::from_reader(Cursor::new(buf)).unwrap();

            the_plist.parent_whole_disk
        };

        let volume_label = "Nix Store".into();

        Ok(InstallPlan {
            planner: self.clone().into(),
            actions: vec![
                // Create Volume step:
                //
                // setup_Synthetic -> create_synthetic_objects
                // Unmount -> create_volume -> Setup_fstab -> maybe encrypt_volume -> launchctl bootstrap -> launchctl kickstart -> await_volume -> maybe enableOwnership
                CreateApfsVolume::plan(root_disk, volume_label, false, None)
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ProvisionNix::plan(self.settings.clone())
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ConfigureNix::plan(self.settings)
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                KickstartLaunchctlService::plan("system/org.nixos.nix-daemon".into())
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
            ],
        })
    }
}

impl Into<BuiltinPlanner> for DarwinMulti {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::DarwinMulti(self)
    }
}
