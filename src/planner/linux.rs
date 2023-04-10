use crate::{
    action::{
        base::{CreateDirectory, RemoveDirectory},
        common::{ConfigureInitService, ConfigureNix, ProvisionNix},
        StatefulAction,
    },
    error::HasExpectedErrors,
    planner::{Planner, PlannerError},
    settings::CommonSettings,
    settings::{InitSettings, InitSystem, InstallSettingsError},
    Action, BuiltinPlanner,
};
use std::{collections::HashMap, path::Path};
use tokio::process::Command;

use super::ShellProfileLocations;

/// A planner for Linux installs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct Linux {
    #[cfg_attr(feature = "cli", clap(flatten))]
    pub settings: CommonSettings,
    #[cfg_attr(feature = "cli", clap(flatten))]
    pub init: InitSettings,
}

#[async_trait::async_trait]
#[typetag::serde(name = "linux")]
impl Planner for Linux {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: CommonSettings::default().await?,
            init: InitSettings::default().await?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        check_not_nixos()?;

        check_nix_not_already_installed().await?;

        check_not_wsl1()?;

        check_not_selinux().await?;

        if self.init.init == InitSystem::Systemd && self.init.start_daemon {
            check_systemd_active()?;
        }

        Ok(vec![
            CreateDirectory::plan("/nix", None, None, 0o0755, true)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ProvisionNix::plan(&self.settings.clone())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ConfigureNix::plan(ShellProfileLocations::default(), &self.settings)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ConfigureInitService::plan(
                self.init.init,
                self.init.start_daemon,
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
        let Self { settings, init } = self;
        let mut map = HashMap::default();

        map.extend(settings.settings()?.into_iter());
        map.extend(init.settings()?.into_iter());

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

impl Into<BuiltinPlanner> for Linux {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::Linux(self)
    }
}

// If on NixOS, running `nix_installer` is pointless
fn check_not_nixos() -> Result<(), PlannerError> {
    // NixOS always sets up this file as part of setting up /etc itself: https://github.com/NixOS/nixpkgs/blob/bdd39e5757d858bd6ea58ed65b4a2e52c8ed11ca/nixos/modules/system/etc/setup-etc.pl#L145
    if Path::new("/etc/NIXOS").exists() {
        return Err(PlannerError::NixOs);
    }
    Ok(())
}

fn check_not_wsl1() -> Result<(), PlannerError> {
    // Detection strategies: https://patrickwu.space/wslconf/
    if std::env::var("WSL_DISTRO_NAME").is_ok() && std::env::var("WSL_INTEROP").is_err() {
        return Err(PlannerError::Wsl1);
    }
    Ok(())
}

async fn check_not_selinux() -> Result<(), PlannerError> {
    // We currently do not support SELinux
    match Command::new("getenforce").output().await {
        Ok(output) => {
            let stdout_string = String::from_utf8(output.stdout).map_err(PlannerError::Utf8)?;
            tracing::trace!(getenforce_stdout = stdout_string, "SELinux detected");
            match stdout_string.trim() {
                "Enforcing" => return Err(PlannerError::SelinuxEnforcing),
                _ => (),
            }
        },
        // The device doesn't have SELinux set up
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        // Some unknown error
        Err(e) => {
            tracing::warn!(error = ?e, "Got an error checking for SELinux setting, this install may fail if SELinux is set to `Enforcing`")
        },
    }

    Ok(())
}

async fn check_nix_not_already_installed() -> Result<(), PlannerError> {
    // For now, we don't try to repair the user's Nix install or anything special.
    if let Ok(_) = Command::new("nix-env")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .status()
        .await
    {
        return Err(PlannerError::NixExists);
    }

    Ok(())
}

fn check_systemd_active() -> Result<(), PlannerError> {
    if !Path::new("/run/systemd/system").exists() {
        if std::env::var("WSL_DISTRO_NAME").is_ok() {
            return Err(LinuxErrorKind::Wsl2SystemdNotActive)?;
        } else {
            return Err(LinuxErrorKind::SystemdNotActive)?;
        }
    }

    Ok(())
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum LinuxErrorKind {
    #[error(
        "\
        systemd was not active.\n\
        \n\
        If it will be started later consider, passing `--no-start-daemon`.\n\
        \n\
        To use a `root`-only Nix install, consider passing `--init none`."
    )]
    SystemdNotActive,
    #[error(
        "\
        systemd was not active.\n\
        \n\
        On WSL2, systemd is not enabled by default. Consider enabling it by adding it to your `/etc/wsl.conf` with `echo -e '[boot]\\nsystemd=true'` then restarting WSL2 with `wsl.exe --shutdown` and re-entering the WSL shell. For more information, see https://devblogs.microsoft.com/commandline/systemd-support-is-now-available-in-wsl/.\n\
        \n\
        If it will be started later consider, passing `--no-start-daemon`.\n\
        \n\
        To use a `root`-only Nix install, consider passing `--init none`."
    )]
    Wsl2SystemdNotActive,
}

impl HasExpectedErrors for LinuxErrorKind {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            LinuxErrorKind::SystemdNotActive => Some(Box::new(self)),
            LinuxErrorKind::Wsl2SystemdNotActive => Some(Box::new(self)),
        }
    }
}

impl From<LinuxErrorKind> for PlannerError {
    fn from(v: LinuxErrorKind) -> PlannerError {
        PlannerError::Custom(Box::new(v))
    }
}
