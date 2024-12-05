use std::{collections::HashMap, io::Cursor, path::PathBuf};

#[cfg(feature = "cli")]
use clap::ArgAction;
use tokio::process::Command;
use which::which;

use super::ShellProfileLocations;
use crate::action::common::provision_nix::NIX_STORE_LOCATION;
use crate::planner::HasExpectedErrors;

mod profile_queries;
mod profiles;

use crate::action::common::ConfigureDeterminateNixdInitService;
use crate::os::darwin::diskutil::DiskUtilList;
use crate::{
    action::{
        base::RemoveDirectory,
        common::{
            ConfigureNix, ConfigureUpstreamInitService, CreateUsersAndGroups,
            ProvisionDeterminateNixd, ProvisionNix,
        },
        macos::{
            ConfigureRemoteBuilding, CreateDeterminateNixVolume, CreateNixHookService,
            CreateNixVolume, SetTmutilExclusions,
        },
        StatefulAction,
    },
    execute_command,
    os::darwin::DiskUtilInfoOutput,
    planner::{Planner, PlannerError},
    settings::InstallSettingsError,
    settings::{determinate_nix_settings, CommonSettings, InitSystem},
    Action, BuiltinPlanner,
};

/// A planner for MacOS (Darwin) systems
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

    /// On AWS, put the Nix Store volume on the EC2 instances' instance store volume.
    ///
    /// WARNING: Using the instance store volume means the machine must never be Stopped in AWS.
    /// If the instance is Stopped, the instance store volume is erased, and the installation is broken.
    /// The machine can be safely rebooted.
    ///
    /// Using the instance store volume bypasses the interactive "enable full disk access" step.
    /// Without this flag, installations on macOS on EC2 will require manual, graphical intervention when first installed to grant Full Disk Access.
    ///
    /// Setting this option:
    ///  * Requires passing --determinate due to complications of AWS's deployment of macOS.
    ///  * Sets --root-disk to an auto-detected disk
    #[cfg_attr(
        feature = "cli",
        clap(long, default_value = "false", requires = "determinate_nix")
    )]
    pub use_ec2_instance_store: bool,
}

async fn default_root_disk() -> Result<String, PlannerError> {
    let buf = execute_command(
        Command::new("/usr/sbin/diskutil")
            .args(["info", "-plist", "/"])
            .stdin(std::process::Stdio::null()),
    )
    .await
    .map_err(|e| PlannerError::Custom(Box::new(e)))?
    .stdout;
    let the_plist: DiskUtilInfoOutput = plist::from_reader(Cursor::new(buf))?;

    Ok(the_plist.parent_whole_disk)
}

async fn default_internal_root_disk() -> Result<Option<String>, PlannerError> {
    let buf = execute_command(
        Command::new("/usr/sbin/diskutil")
            .args(["list", "-plist", "internal", "virtual"])
            .stdin(std::process::Stdio::null()),
    )
    .await
    .map_err(|e| PlannerError::Custom(Box::new(e)))?
    .stdout;
    let the_plist: DiskUtilList = plist::from_reader(Cursor::new(buf))?;

    let mut disks = the_plist
        .all_disks_and_partitions
        .into_iter()
        .filter(|disk| !disk.os_internal)
        .collect::<Vec<_>>();

    disks.sort_by_key(|d| d.size_bytes);

    Ok(disks.pop().map(|d| d.device_identifier))
}

#[async_trait::async_trait]
#[typetag::serde(name = "macos")]
impl Planner for Macos {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: CommonSettings::default().await?,
            use_ec2_instance_store: false,
            root_disk: Some(default_root_disk().await?),
            case_sensitive: false,
            encrypt: None,
            volume_label: "Nix Store".into(),
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        if self.use_ec2_instance_store && !self.settings.determinate_nix {
            return Err(PlannerError::Ec2InstanceStoreRequiresDeterminateNix);
        }

        let root_disk = match &self.root_disk {
            root_disk @ Some(_) => root_disk.clone(),
            None => {
                if self.use_ec2_instance_store {
                    default_internal_root_disk().await?
                } else {
                    Some(default_root_disk().await?)
                }
            },
        };

        // The encrypt variable isn't used in Determinate Nix since we have our own plan step for it,
        // however this match accounts for Determinate Nix so the receipt indicates encrypt: true.
        // This is a goofy thing to do, but it is in an attempt to make a more globally coherent plan / receipt.
        let encrypt = match (self.settings.determinate_nix, self.encrypt) {
            (true, _) => true,
            (false, Some(choice)) => {
                if let Some(diskutil_info) =
                    crate::action::macos::get_disk_info_for_label(&self.volume_label)
                        .await
                        .ok()
                        .flatten()
                {
                    if diskutil_info.file_vault {
                        tracing::warn!("Existing volume was encrypted with FileVault, forcing `encrypt` to true");
                        true
                    } else {
                        choice
                    }
                } else {
                    choice
                }
            },
            (false, None) => {
                let root_disk_is_encrypted = {
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

                    stdout_trimmed == "true"
                };

                let existing_store_volume_is_encrypted = {
                    if let Some(diskutil_info) =
                        crate::action::macos::get_disk_info_for_label(&self.volume_label)
                            .await
                            .ok()
                            .flatten()
                    {
                        diskutil_info.file_vault
                    } else {
                        false
                    }
                };

                root_disk_is_encrypted || existing_store_volume_is_encrypted
            },
        };

        let mut plan = vec![];

        if self.settings.determinate_nix {
            plan.push(
                ProvisionDeterminateNixd::plan()
                    .await
                    .map_err(PlannerError::Action)?
                    .boxed(),
            );
        }

        if self.settings.determinate_nix {
            println!("Creating determinate nix volume {0}", self.volume_label);
            println!(
                "Installer volume label: {}",
                std::env::var("NIX_INSTALLER_VOLUME_LABEL").unwrap_or_default()
            );
            plan.push(
                CreateDeterminateNixVolume::plan(
                    root_disk.unwrap(), /* We just ensured it was populated */
                    self.volume_label.clone(),
                    self.case_sensitive,
                    self.settings.force,
                    self.use_ec2_instance_store,
                )
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            );
        } else {
            plan.push(
                CreateNixVolume::plan(
                    root_disk.unwrap(), /* We just ensured it was populated */
                    self.volume_label.clone(),
                    self.case_sensitive,
                    encrypt,
                )
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            );
        }

        plan.push(
            ProvisionNix::plan(&self.settings)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );
        // Auto-allocate uids is broken on Mac. Tools like `whoami` don't work.
        // e.g. https://github.com/NixOS/nix/issues/8444
        plan.push(
            CreateUsersAndGroups::plan(self.settings.clone())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );
        plan.push(
            SetTmutilExclusions::plan(vec![
                PathBuf::from(NIX_STORE_LOCATION),
                PathBuf::from("/nix/var"),
            ])
            .await
            .map_err(PlannerError::Action)?
            .boxed(),
        );
        plan.push(
            ConfigureNix::plan(
                ShellProfileLocations::default(),
                &self.settings,
                self.settings.determinate_nix.then(determinate_nix_settings),
            )
            .await
            .map_err(PlannerError::Action)?
            .boxed(),
        );
        plan.push(
            ConfigureRemoteBuilding::plan()
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );

        if self.settings.modify_profile {
            plan.push(
                CreateNixHookService::plan()
                    .await
                    .map_err(PlannerError::Action)?
                    .boxed(),
            );
        }

        if self.settings.determinate_nix {
            plan.push(
                ConfigureDeterminateNixdInitService::plan(InitSystem::Launchd, true)
                    .await
                    .map_err(PlannerError::Action)?
                    .boxed(),
            );
        } else {
            plan.push(
                ConfigureUpstreamInitService::plan(InitSystem::Launchd, true)
                    .await
                    .map_err(PlannerError::Action)?
                    .boxed(),
            );
        }
        plan.push(
            RemoveDirectory::plan(crate::settings::SCRATCH_DIR)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );

        Ok(plan)
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self {
            settings,
            encrypt,
            volume_label,
            case_sensitive,
            root_disk,
            use_ec2_instance_store,
        } = self;
        let mut map = HashMap::default();

        map.extend(settings.settings()?);
        map.insert("volume_encrypt".into(), serde_json::to_value(encrypt)?);
        map.insert("volume_label".into(), serde_json::to_value(volume_label)?);
        map.insert("root_disk".into(), serde_json::to_value(root_disk)?);
        map.insert(
            "use_ec2_instance_store".into(),
            serde_json::to_value(use_ec2_instance_store)?,
        );
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
            self.settings.diagnostic_attribution.clone(),
            self.settings.diagnostic_endpoint.clone(),
            self.typetag_name().into(),
            self.configured_settings()
                .await?
                .into_keys()
                .collect::<Vec<_>>(),
            self.settings.ssl_cert_file.clone(),
        )?)
    }

    async fn platform_check(&self) -> Result<(), PlannerError> {
        use target_lexicon::OperatingSystem;
        match target_lexicon::OperatingSystem::host() {
            OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => Ok(()),
            host_os => Err(PlannerError::IncompatibleOperatingSystem {
                planner: self.typetag_name(),
                host_os,
            }),
        }
    }

    async fn pre_uninstall_check(&self) -> Result<(), PlannerError> {
        check_nix_darwin_not_installed().await?;

        Ok(())
    }

    async fn pre_install_check(&self) -> Result<(), PlannerError> {
        check_suis().await?;
        check_not_running_in_rosetta()?;

        Ok(())
    }
}

impl From<Macos> for BuiltinPlanner {
    fn from(val: Macos) -> Self {
        BuiltinPlanner::Macos(val)
    }
}

async fn check_nix_darwin_not_installed() -> Result<(), PlannerError> {
    let has_darwin_rebuild = which("darwin-rebuild").is_ok();
    let has_darwin_option = which("darwin-option").is_ok();

    let activate_system_present = Command::new("launchctl")
        .arg("print")
        .arg("system/org.nixos.activate-system")
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|v| v.success())
        .unwrap_or(false);

    if activate_system_present || has_darwin_rebuild || has_darwin_option {
        return Err(MacosError::UninstallNixDarwin).map_err(|e| PlannerError::Custom(Box::new(e)));
    };

    Ok(())
}

fn check_not_running_in_rosetta() -> Result<(), PlannerError> {
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

async fn check_suis() -> Result<(), PlannerError> {
    let policies: profiles::Policies = match profiles::load().await {
        Ok(pol) => pol,
        Err(e) => {
            tracing::warn!(
                "Skipping SystemUIServer checks: failed to load profile data: {:?}",
                e
            );
            return Ok(());
        },
    };

    let blocks: Vec<_> = profile_queries::blocks_internal_mounting(&policies)
        .into_iter()
        .map(|blocking_policy| blocking_policy.display())
        .collect();

    let error: String = match &blocks[..] {
        [] => {
            return Ok(());
        },
        [block] => format!(
            "The following macOS configuration profile includes a 'Restrictions - Media' policy, which interferes with the Nix Store volume:\n\n{}\n\nSee https://determinate.systems/solutions/macos-internal-disk-policy",
            block
        ),
        blocks => {
            format!(
                "The following macOS configuration profiles include a 'Restrictions - Media' policy, which interferes with the Nix Store volume:\n\n{}\n\nSee https://determinate.systems/solutions/macos-internal-disk-policy",
                blocks.join("\n\n")
            )
        },
    };

    Err(MacosError::BlockedBySystemUIServerPolicy(error))
        .map_err(|e| PlannerError::Custom(Box::new(e)))
}

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum MacosError {
    #[error("`nix-darwin` installation detected, it must be removed before uninstalling Nix. Please refer to https://github.com/LnL7/nix-darwin#uninstalling for instructions how to uninstall `nix-darwin`.")]
    UninstallNixDarwin,

    #[error("{0}")]
    BlockedBySystemUIServerPolicy(String),
}

impl HasExpectedErrors for MacosError {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            this @ MacosError::UninstallNixDarwin => Some(Box::new(this)),
            this @ MacosError::BlockedBySystemUIServerPolicy(_) => Some(Box::new(this)),
        }
    }
}

#[cfg(all(test, feature = "cli"))]
mod tests {
    use super::*;
    use clap::Parser;

    #[tokio::test]
    async fn test_volume_label() {
        // Test default value
        std::env::remove_var("NIX_INSTALLER_VOLUME_LABEL");
        let macos = Macos::parse_from(Vec::<String>::new());
        assert_eq!(
            macos.volume_label, "Nix Store",
            "Default value should be 'Nix Store'"
        );

        // Test env var override
        std::env::set_var("NIX_INSTALLER_VOLUME_LABEL", "Custom Volume");
        let macos = Macos::parse_from(Vec::<String>::new());
        assert_eq!(
            macos.volume_label, "Custom Volume",
            "Environment variable should override default"
        );

        // Cleanup
        std::env::remove_var("NIX_INSTALLER_VOLUME_LABEL");
    }
}
