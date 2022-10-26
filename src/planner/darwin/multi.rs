use std::io::Cursor;

use clap::ArgAction;
use tokio::process::Command;

use crate::{
    actions::{
        base::darwin::KickstartLaunchctlService,
        meta::{darwin::CreateApfsVolume, ConfigureNix, ProvisionNix},
    },
    execute_command,
    os::darwin::DiskUtilOutput,
    planner::{BuiltinPlannerError, Plannable},
    BuiltinPlanner, CommonSettings, InstallPlan,
};

#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct DarwinMulti {
    #[clap(flatten)]
    settings: CommonSettings,
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        env = "HARMONIC_VOLUME_ENCRYPT"
    )]
    volume_encrypt: bool,
    #[clap(long, default_value = "Nix Store", env = "HARMONIC_VOLUME_LABEL")]
    volume_label: String,
    #[clap(long, env = "HARMONIC_ROOT_DISK")]
    root_disk: Option<String>,
}

async fn default_root_disk() -> Result<String, BuiltinPlannerError> {
    let buf = execute_command(Command::new("/usr/sbin/diskutil").args(["info", "-plist", "/"]))
        .await
        .unwrap()
        .stdout;
    let the_plist: DiskUtilOutput = plist::from_reader(Cursor::new(buf))?;

    Ok(the_plist.parent_whole_disk)
}

#[async_trait::async_trait]
impl Plannable for DarwinMulti {
    const DISPLAY_STRING: &'static str = "Darwin Multi-User";
    const SLUG: &'static str = "darwin-multi";
    type Error = BuiltinPlannerError;

    async fn default() -> Result<Self, Self::Error> {
        Ok(Self {
            settings: CommonSettings::default()?,
            root_disk: Some(default_root_disk().await?),
            volume_encrypt: false,
            volume_label: "Nix Store".into(),
        })
    }

    async fn plan(self) -> Result<crate::InstallPlan, Self::Error> {
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
                Box::new(CreateApfsVolume::plan(root_disk, volume_label, false, None).await?),
                Box::new(ProvisionNix::plan(self.settings.clone()).await?),
                Box::new(ConfigureNix::plan(self.settings).await?),
                Box::new(
                    KickstartLaunchctlService::plan("system/org.nixos.nix-daemon".into()).await?,
                ),
            ],
        })
    }
}

impl Into<BuiltinPlanner> for DarwinMulti {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::DarwinMulti(self)
    }
}
