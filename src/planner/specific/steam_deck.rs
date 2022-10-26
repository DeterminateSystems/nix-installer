use crate::{
    actions::{
        base::{CreateDirectory, StartSystemdUnit},
        meta::{CreateSystemdSysext, ProvisionNix},
        Action, ActionError,
    },
    planner::{Plannable, PlannerError},
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

    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(self) -> Result<crate::InstallPlan, PlannerError> {
        Ok(InstallPlan {
            planner: self.clone().into(),
            actions: vec![
                CreateSystemdSysext::plan("/var/lib/extensions")
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                CreateDirectory::plan("/nix", None, None, 0o0755, true)
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ProvisionNix::plan(self.settings.clone())
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                StartSystemdUnit::plan("nix-daemon.socket".into())
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
            ],
        })
    }
}

impl Into<BuiltinPlanner> for SteamDeck {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::SteamDeck(self)
    }
}
