use crate::{
    actions::{
        base::{CreateDirectory, StartSystemdUnit},
        meta::{CreateSystemdSysext, ProvisionNix},
    },
    planner::{BuiltinPlannerError, Plannable},
    BuiltinPlanner, CommonSettings, InstallPlan,
};

#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct SteamDeck {
    #[clap(flatten)]
    settings: CommonSettings,
}

#[async_trait::async_trait]
impl Plannable for SteamDeck {
    const DISPLAY_STRING: &'static str = "Steam Deck (x86_64 Linux Multi-User)";
    const SLUG: &'static str = "steam-deck";
    type Error = BuiltinPlannerError;

    async fn default() -> Result<Self, Self::Error> {
        Ok(Self {
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(self) -> Result<crate::InstallPlan, Self::Error> {
        Ok(InstallPlan {
            planner: self.clone().into(),
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
