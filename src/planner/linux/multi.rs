use crate::{
    action::{
        base::CreateDirectory,
        common::{ConfigureNix, ProvisionNix},
        linux::StartSystemdUnit,
        StatefulAction,
    },
    planner::{Planner, PlannerError},
    settings::CommonSettings,
    settings::InstallSettingsError,
    Action, BoxableError, BuiltinPlanner,
};
use std::{collections::HashMap, path::Path};

/// A planner for Linux multi-user installs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct LinuxMulti {
    #[cfg_attr(feature = "cli", clap(flatten))]
    pub settings: CommonSettings,
}

#[async_trait::async_trait]
#[typetag::serde(name = "linux-multi")]
impl Planner for LinuxMulti {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        // If on NixOS, running `harmonic` is pointless
        // NixOS always sets up this file as part of setting up /etc itself: https://github.com/NixOS/nixpkgs/blob/bdd39e5757d858bd6ea58ed65b4a2e52c8ed11ca/nixos/modules/system/etc/setup-etc.pl#L145
        if Path::new("/etc/NIXOS").exists() {
            return Err(PlannerError::Custom(Box::new(LinuxMultiError::NixOs)));
        }

        // For now, we don't try to repair the user's Nix install or anything special.
        if let Ok(_) = tokio::process::Command::new("nix-env")
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .status()
            .await
        {
            return Err(PlannerError::Custom(Box::new(LinuxMultiError::NixExists)));
        }

        Ok(vec![
            CreateDirectory::plan("/nix", None, None, 0o0755, true)
                .await
                .map_err(|e| PlannerError::Action(e.boxed()))?
                .boxed(),
            ProvisionNix::plan(&self.settings.clone())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ConfigureNix::plan(&self.settings)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            StartSystemdUnit::plan("nix-daemon.socket".to_string())
                .await
                .map_err(|v| PlannerError::Action(v.into()))?
                .boxed(),
        ])
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self { settings } = self;
        let mut map = HashMap::default();

        map.extend(settings.settings()?.into_iter());

        Ok(map)
    }
}

impl Into<BuiltinPlanner> for LinuxMulti {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::LinuxMulti(self)
    }
}

#[derive(thiserror::Error, Debug)]
enum LinuxMultiError {
    #[error("NixOS already has Nix installed")]
    NixOs,
    #[error("`nix` is already a valid command, so it is installed")]
    NixExists,
    #[error("Error planning action")]
    Action(
        #[source]
        #[from]
        Box<dyn std::error::Error + Send + Sync>,
    ),
}
