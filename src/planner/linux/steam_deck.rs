use std::collections::HashMap;

use crate::{
    action::{
        common::{ConfigureNix, CreateDirectory, ProvisionNix},
        linux::{
            CreateSystemdSysext, StartSystemdUnit, SteamosReadonly, SteamosReadonlyError,
            SystemdSysextMerge, SystemdSysextMergeError,
        },
    },
    planner::Planner,
    BuiltinPlanner, CommonSettings, InstallPlan,
};
use clap::ArgAction;

#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct SteamDeck {
    #[clap(flatten)]
    pub settings: CommonSettings,
}

#[async_trait::async_trait]
#[typetag::serde(name = "steam-deck")]
impl Planner for SteamDeck {
    async fn default() -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Self {
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(self) -> Result<crate::InstallPlan, Box<dyn std::error::Error + Sync + Send>> {
        Ok(InstallPlan {
            planner: Box::new(self.clone()),
            actions: vec![
                Box::new(SteamosReadonly::plan(false).await?),
                Box::new(CreateDirectory::plan("/nix", None, None, None, true).await?),
                Box::new(SteamosReadonly::plan(true).await?),
                Box::new(
                    CreateDirectory::plan("/var/lib/extensions/", None, None, None, true).await?,
                ),
                Box::new(CreateDirectory::plan("/home/nix", None, None, None, true).await?),
                Box::new(CreateSystemdSysext::plan("/var/lib/extensions/nix", "/home/nix").await?),
                Box::new(SystemdSysextMerge::plan().await?),
                Box::new(StartSystemdUnit::plan("nix.mount").await?),
                Box::new(ProvisionNix::plan(self.settings.clone()).await?),
                Box::new(ConfigureNix::plan(self.settings).await?),
                Box::new(StartSystemdUnit::plan("nix-daemon.socket".to_string()).await?),
            ],
        })
    }

    fn describe(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, Box<dyn std::error::Error + Sync + Send>> {
        let Self { settings } = self;
        let mut map = HashMap::default();

        map.extend(settings.describe()?.into_iter());

        Ok(map)
    }
}

impl Into<BuiltinPlanner> for SteamDeck {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::SteamDeck(self)
    }
}
