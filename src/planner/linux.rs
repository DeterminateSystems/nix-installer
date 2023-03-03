use crate::{
    action::{
        base::CreateDirectory,
        common::{ConfigureInitService, ConfigureNix, ProvisionNix},
        StatefulAction,
    },
    planner::{Planner, PlannerError},
    settings::CommonSettings,
    settings::{InitSettings, InstallSettingsError},
    Action, BuiltinPlanner,
};
use std::{collections::HashMap, path::Path};
use tokio::process::Command;

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
        // If on NixOS, running `nix_installer` is pointless
        // NixOS always sets up this file as part of setting up /etc itself: https://github.com/NixOS/nixpkgs/blob/bdd39e5757d858bd6ea58ed65b4a2e52c8ed11ca/nixos/modules/system/etc/setup-etc.pl#L145
        if Path::new("/etc/NIXOS").exists() {
            return Err(PlannerError::NixOs);
        }

        if std::env::var("WSL_DISTRO_NAME").is_ok() && std::env::var("WSL_INTEROP").is_err() {
            return Err(PlannerError::Wsl1);
        }

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

        // For now, we don't try to repair the user's Nix install or anything special.
        if let Ok(_) = Command::new("nix-env")
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .status()
            .await
        {
            return Err(PlannerError::NixExists);
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
            ConfigureNix::plan(&self.settings)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ConfigureInitService::plan(self.init.init, self.init.start_daemon)
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

    #[cfg(feature = "diagnostics")]
    async fn diagnostic_data(&self) -> Result<crate::diagnostics::DiagnosticData, PlannerError> {
        Ok(crate::diagnostics::DiagnosticData::new(
            self.settings.diagnostic_endpoint.clone(),
            self.typetag_name().into(),
            self.configured_settings().await?,
        ))
    }
}

impl Into<BuiltinPlanner> for Linux {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::Linux(self)
    }
}
