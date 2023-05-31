use std::{collections::HashMap, io::Cursor, path::PathBuf};

#[cfg(feature = "cli")]
use clap::ArgAction;
use tokio::process::Command;

use super::ShellProfileLocations;

use crate::{
    action::{
        base::RemoveDirectory,
        common::{ConfigureInitService, ConfigureNix, ProvisionNix},
        macos::{CreateNixVolume, SetTmutilExclusions},
        StatefulAction,
    },
    execute_command,
    os::darwin::DiskUtilInfoOutput,
    planner::{Planner, PlannerError},
    settings::InstallSettingsError,
    settings::{CommonSettings, InitSystem},
    Action, BuiltinPlanner,
};

/// A planner for MacOS (Darwin) installs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct Macos {
    #[cfg_attr(feature = "cli", clap(flatten))]
    pub settings: CommonSettings,

    /// Force encryption on the volume
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            action(ArgAction::Set),
            default_value = "false",
            env = "NIX_INSTALLER_ENCRYPT"
        )
    )]
    pub encrypt: Option<bool>,
    /// Use a case sensitive volume
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            action(ArgAction::SetTrue),
            default_value = "false",
            env = "NIX_INSTALLER_CASE_SENSITIVE"
        )
    )]
    pub case_sensitive: bool,
    /// The label for the created APFS volume
    #[cfg_attr(
        feature = "cli",
        clap(long, default_value = "Nix Store", env = "NIX_INSTALLER_VOLUME_LABEL")
    )]
    pub volume_label: String,
    /// The root disk of the target
    #[cfg_attr(feature = "cli", clap(long, env = "NIX_INSTALLER_ROOT_DISK"))]
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
    let the_plist: DiskUtilInfoOutput = plist::from_reader(Cursor::new(buf))?;

    Ok(the_plist.parent_whole_disk)
}

#[async_trait::async_trait]
#[typetag::serde(name = "macos")]
impl Planner for Macos {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: CommonSettings::default().await?,
            root_disk: Some(default_root_disk().await?),
            case_sensitive: false,
            encrypt: None,
            volume_label: "Nix Store".into(),
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        ensure_not_running_in_rosetta().await?;

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
                let the_plist: DiskUtilInfoOutput = plist::from_reader(Cursor::new(buf)).unwrap();

                Some(the_plist.parent_whole_disk)
            },
        };

        let encrypt = if self.encrypt == None {
            let output = Command::new("/usr/bin/fdesetup")
                .arg("isactive")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .process_group(0)
                .output()
                .await
                .map_err(|e| PlannerError::Custom(Box::new(e)))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stdout_trimmed = stdout.trim();
            if stdout_trimmed == "true" {
                true
            } else {
                false
            }
        } else {
            false
        };

        Ok(vec![
            CreateNixVolume::plan(
                root_disk.unwrap(), /* We just ensured it was populated */
                self.volume_label.clone(),
                false,
                encrypt,
            )
            .await
            .map_err(PlannerError::Action)?
            .boxed(),
            ProvisionNix::plan(&self.settings)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            SetTmutilExclusions::plan(vec![PathBuf::from("/nix/store"), PathBuf::from("/nix/var")])
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ConfigureNix::plan(ShellProfileLocations::default(), &self.settings)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ConfigureInitService::plan(
                InitSystem::Launchd,
                true,
                self.settings.ssl_cert_file.clone(),
            )
            .await
            .map_err(PlannerError::Action)?
            .boxed(),
            RemoveDirectory::plan(crate::settings::SCRATCH_DIR)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
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

    async fn configured_settings(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, PlannerError> {
        let default = Self::default().await?.settings()?;
        let configured = self.settings()?;

        let mut settings: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, value) in configured.iter() {
            if default.get(key) != Some(value) {
                settings.insert(key.clone(), value.clone());
            }
        }

        Ok(settings)
    }

    #[cfg(feature = "diagnostics")]
    async fn diagnostic_data(&self) -> Result<crate::diagnostics::DiagnosticData, PlannerError> {
        Ok(crate::diagnostics::DiagnosticData::new(
            self.settings.diagnostic_endpoint.clone(),
            self.typetag_name().into(),
            self.configured_settings()
                .await?
                .into_keys()
                .collect::<Vec<_>>(),
            self.settings.ssl_cert_file.clone(),
        )?)
    }
}

impl Into<BuiltinPlanner> for Macos {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::Macos(self)
    }
}

async fn ensure_not_running_in_rosetta() -> Result<(), PlannerError> {
    use sysctl::{Ctl, Sysctl};
    const CTLNAME: &str = "sysctl.proc_translated";

    match Ctl::new(CTLNAME) {
        // This Mac doesn't have Rosetta!
        Err(sysctl::SysctlError::NotFound(_)) => (),
        Err(e) => Err(e)?,
        Ok(ctl) => {
            let str_val = ctl.value_string()?;

            if str_val == "1" {
                return Err(PlannerError::RosettaDetected);
            }
        },
    }

    Ok(())
}
