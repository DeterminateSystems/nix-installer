use std::{collections::HashMap, io::Cursor};

use clap::ArgAction;
use tokio::process::Command;

use crate::{
    action::{
        common::{ConfigureNix, ProvisionNix},
        darwin::{CreateApfsVolume, KickstartLaunchctlService},
    },
    execute_command,
    os::darwin::DiskUtilOutput,
    planner::{Planner, PlannerError},
    settings::CommonSettings,
    settings::InstallSettingsError,
    Action, BuiltinPlanner,
};

/// A planner for MacOS (Darwin) multi-user installs
#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct DarwinMulti {
    #[clap(flatten)]
    pub settings: CommonSettings,
    /// Force encryption on the volume
    #[clap(
        long,
        action(ArgAction::Set),
        default_value = "false",
        env = "HARMONIC_ENCRYPT"
    )]
    pub encrypt: Option<bool>,
    /// Use a case sensitive volume
    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        env = "HARMONIC_CASE_SENSITIVE"
    )]
    pub case_sensitive: bool,
    /// The label for the created APFS volume
    #[clap(long, default_value = "Nix Store", env = "HARMONIC_VOLUME_LABEL")]
    pub volume_label: String,
    /// The root disk of the target
    #[clap(long, env = "HARMONIC_ROOT_DISK")]
    pub root_disk: Option<String>,
}

async fn default_root_disk() -> Result<String, PlannerError> {
    let buf = execute_command(
        Command::new("/usr/sbin/diskutil")
            .args(["info", "-plist", "/"])
            .stdin(std::process::Stdio::null()),
    )
    .await
    .unwrap()
    .stdout;
    let the_plist: DiskUtilOutput = plist::from_reader(Cursor::new(buf))?;

    Ok(the_plist.parent_whole_disk)
}

#[async_trait::async_trait]
#[typetag::serde(name = "darwin-multi")]
impl Planner for DarwinMulti {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: CommonSettings::default()?,
            root_disk: Some(default_root_disk().await?),
            case_sensitive: false,
            encrypt: None,
            volume_label: "Nix Store".into(),
        })
    }

    async fn plan(&self) -> Result<Vec<Box<dyn Action>>, PlannerError> {
        let root_disk = match &self.root_disk {
            root_disk @ Some(_) => root_disk.clone(),
            None => {
                let buf = execute_command(
                    Command::new("/usr/sbin/diskutil")
                        .args(["info", "-plist", "/"])
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .unwrap()
                .stdout;
                let the_plist: DiskUtilOutput = plist::from_reader(Cursor::new(buf)).unwrap();

                Some(the_plist.parent_whole_disk)
            },
        };

        let encrypt = if self.encrypt == None {
            Command::new("/usr/bin/fdesetup")
                .arg("isactive")
                .status()
                .await
                .map_err(|e| PlannerError::Custom(Box::new(e)))?
                .code()
                .map(|v| if v == 0 { false } else { true })
                .unwrap_or(false)
        } else {
            false
        };

        Ok(vec![
            // Create Volume step:
            //
            // setup_Synthetic -> create_synthetic_objects
            // Unmount -> create_volume -> Setup_fstab -> maybe encrypt_volume -> launchctl bootstrap -> launchctl kickstart -> await_volume -> maybe enableOwnership
            Box::new(
                CreateApfsVolume::plan(
                    root_disk.unwrap(), /* We just ensured it was populated */
                    self.volume_label.clone(),
                    false,
                    encrypt,
                )
                .await
                .map_err(PlannerError::Action)?,
            ),
            Box::new(
                ProvisionNix::plan(&self.settings)
                    .await
                    .map_err(PlannerError::Action)?,
            ),
            Box::new(
                ConfigureNix::plan(&self.settings)
                    .await
                    .map_err(PlannerError::Action)?,
            ),
            Box::new(
                KickstartLaunchctlService::plan("system/org.nixos.nix-daemon".into())
                    .await
                    .map_err(PlannerError::Action)?,
            ),
        ])
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self {
            settings,
            encrypt,
            volume_label,
            case_sensitive,
            root_disk,
        } = self;
        let mut map = HashMap::default();

        map.extend(settings.settings()?.into_iter());
        map.insert("volume_encrypt".into(), serde_json::to_value(encrypt)?);
        map.insert("volume_label".into(), serde_json::to_value(volume_label)?);
        map.insert("root_disk".into(), serde_json::to_value(root_disk)?);
        map.insert(
            "case_sensitive".into(),
            serde_json::to_value(case_sensitive)?,
        );

        Ok(map)
    }
}

impl Into<BuiltinPlanner> for DarwinMulti {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::DarwinMulti(self)
    }
}
