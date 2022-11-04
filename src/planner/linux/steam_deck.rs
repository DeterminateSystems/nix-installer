use std::collections::HashMap;

use crate::{
    action::{
        common::{ConfigureNix, CreateDirectory, ProvisionNix},
        linux::{CreateSystemdSysext, StartSystemdUnit, SteamosReadonly, SteamosReadonlyError},
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
        let sysext = "/var/lib/extensions/nix";
        Ok(InstallPlan {
            planner: Box::new(self.clone()),
            actions: vec![
                Box::new(
                    CreateDirectory::plan("/var/lib/extensions/", None, None, None, true).await?,
                ),
                Box::new(CreateDirectory::plan("/home/nix", None, None, None, true).await?),
                Box::new(CreateSystemdSysext::plan(sysext, "/home/nix").await?),
                Box::new(StartSystemdUnit::plan("systemd-sysext.service".to_string()).await?), // TODO: We should not disable this during uninstall if it's already on
                Box::new(StartSystemdUnit::plan("nix.mount").await?),
                Box::new(ProvisionNix::plan(self.settings.clone()).await?),
                Box::new(ConfigureNix::plan(self.settings, Some(sysext.into())).await?),
                Box::new(StartSystemdUnit::plan("nix-daemon.service".to_string()).await?),
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
