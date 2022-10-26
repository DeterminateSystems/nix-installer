use crate::{
    action::{
        base::{CreateDirectory, StartSystemdUnit},
        meta::{CreateSystemdSysext, ProvisionNix},
    },
    planner::{BuiltinPlannerError, Planner},
    BuiltinPlanner, CommonSettings, InstallPlan,
};

#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct SteamDeck {
    #[clap(flatten)]
    settings: CommonSettings,
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
                Box::new(CreateSystemdSysext::plan("/var/lib/extensions").await?),
                Box::new(CreateDirectory::plan("/nix", None, None, 0o0755, true).await?),
                Box::new(ProvisionNix::plan(self.settings.clone()).await?),
                Box::new(StartSystemdUnit::plan("nix-daemon.socket".into()).await?),
            ],
        })
    }
}

impl Into<BuiltinPlanner> for SteamDeck {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::SteamDeck(self)
    }
}
