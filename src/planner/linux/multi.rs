use crate::{
    action::{
        base::CreateDirectory,
        common::{ConfigureNix, ProvisionNix},
    },
    planner::Planner,
    BuiltinPlanner, CommonSettings, InstallPlan,
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

    async fn plan(self) -> Result<InstallPlan, Box<dyn std::error::Error + Sync + Send>> {
        // If on NixOS, running `harmonic` is pointless
        if Path::new("/etc/NIXOS").exists() {
            return Err(Error::NixOs.into());
        }

        Ok(InstallPlan {
            planner: Box::new(self.clone()),
            actions: vec![
                Box::new(
                    CreateDirectory::plan("/nix", None, None, 0o0755, true)
                        .await
                        .map_err(|v| Error::Action(v.into()))?,
                ),
                Box::new(
                    ProvisionNix::plan(self.settings.clone())
                        .await
                        .map_err(|v| Error::Action(v.into()))?,
                ),
                Box::new(
                    ConfigureNix::plan(self.settings)
                        .await
                        .map_err(|v| Error::Action(v.into()))?,
                ),
            ],
        })
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
    #[error("Error planning action")]
    Action(
        #[source]
        #[from]
        Box<dyn std::error::Error + Send + Sync>,
    ),
}
