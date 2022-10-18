use tokio::process::Command;

use crate::{
    actions::{
        meta::{darwin::CreateApfsVolume, ConfigureNix, ProvisionNix, StartNixDaemon},
        Action, ActionError,
    },
    execute_command,
    planner::{Plannable, PlannerError},
    InstallPlan, Planner,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct DarwinMultiUser;

#[async_trait::async_trait]
impl Plannable for DarwinMultiUser {
    const DISPLAY_STRING: &'static str = "Darwin Multi-User";
    const SLUG: &'static str = "darwin-multi";

    async fn plan(
        settings: crate::InstallSettings,
    ) -> Result<crate::InstallPlan, crate::planner::PlannerError> {
        let root_disk = {
            let root_disk_buf =
                execute_command(Command::new("/usr/sbin/diskutil").args(["info", "-plist", "/"]))
                    .await
                    .unwrap()
                    .stdout;
            let package =
                sxd_document::parser::parse(&String::from_utf8(root_disk_buf).unwrap()).unwrap();

            match sxd_xpath::evaluate_xpath(
                &package.as_document(),
                "/plist/dict/key[text()='ParentWholeDisk']/following-sibling::string[1]/text()",
            )
            .unwrap()
            {
                sxd_xpath::Value::String(s) => s,
                _ => panic!("At the disk i/o!!!"),
            }
        };

        let volume_label = "Nix Store".into();

        Ok(InstallPlan {
            planner: Self.into(),
            settings: settings.clone(),
            actions: vec![
                // Create Volume step:
                //
                // setup_Synthetic -> create_synthetic_objects
                // Unmount -> create_volume -> Setup_fstab -> maybe encrypt_volume -> launchctl bootstrap -> launchctl kickstart -> await_volume -> maybe enableOwnership
                CreateApfsVolume::plan(root_disk, volume_label, false, None)
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ProvisionNix::plan(settings.clone())
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ConfigureNix::plan(settings)
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                StartNixDaemon::plan()
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
            ],
        })
    }
}

impl Into<Planner> for DarwinMultiUser {
    fn into(self) -> Planner {
        Planner::DarwinMultiUser
    }
}
