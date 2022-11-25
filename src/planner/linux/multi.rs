use crate::{
    action::{
        base::CreateDirectory,
        common::{ConfigureNix, ProvisionNix},
    },
    planner::Planner,
    Action, BuiltinPlanner, CommonSettings,
};
use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct LinuxMulti {
    #[clap(flatten)]
    pub settings: CommonSettings,
}

#[async_trait::async_trait]
#[typetag::serde(name = "linux-multi")]
impl Planner for LinuxMulti {
    async fn default() -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Self {
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(&self) -> Result<Vec<Box<dyn Action>>, Box<dyn std::error::Error + Sync + Send>> {
        // If on NixOS, running `harmonic` is pointless
        // NixOS always sets up this file as part of setting up /etc itself: https://github.com/NixOS/nixpkgs/blob/bdd39e5757d858bd6ea58ed65b4a2e52c8ed11ca/nixos/modules/system/etc/setup-etc.pl#L145
        if Path::new("/etc/NIXOS").exists() {
            return Err(Error::NixOs.into());
        }

        // For now, we don't try to repair the user's Nix install or anything special.
        if let Ok(_) = tokio::process::Command::new("nix-env")
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .status()
            .await
        {
            return Err(Error::NixExists.into());
        }

        Ok(vec![
            Box::new(
                CreateDirectory::plan("/nix", None, None, 0o0755, true)
                    .await
                    .map_err(|v| Error::Action(v.into()))?,
            ),
            Box::new(
                ProvisionNix::plan(&self.settings.clone())
                    .await
                    .map_err(|v| Error::Action(v.into()))?,
            ),
            Box::new(
                ConfigureNix::plan(&self.settings)
                    .await
                    .map_err(|v| Error::Action(v.into()))?,
            ),
        ])
    }

    fn settings(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, Box<dyn std::error::Error + Sync + Send>> {
        let Self { settings } = self;
        let mut map = HashMap::default();

        map.extend(settings.describe()?.into_iter());

        Ok(map)
    }
}

impl Into<BuiltinPlanner> for LinuxMulti {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::LinuxMulti(self)
    }
}

#[derive(thiserror::Error, Debug)]
enum Error {
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
