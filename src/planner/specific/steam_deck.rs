use crate::{
    actions::{
        base::{CreateDirectory, StartSystemdUnit},
        meta::{CreateSystemdSysext, ProvisionNix},
        Action, ActionError,
    },
    planner::Plannable,
    InstallPlan, Planner,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SteamDeck;

#[async_trait::async_trait]
impl Plannable for SteamDeck {
    const DISPLAY_STRING: &'static str = "Steam Deck (x86_64 Linux Multi-User)";
    const SLUG: &'static str = "steam-deck";

    async fn plan(
        settings: crate::InstallSettings,
    ) -> Result<crate::InstallPlan, crate::planner::PlannerError> {
        Ok(InstallPlan {
            planner: Self.into(),
            settings: settings.clone(),
            actions: vec![
                CreateSystemdSysext::plan("/var/lib/extensions")
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                CreateDirectory::plan("/nix", None, None, 0o0755, true)
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ProvisionNix::plan(settings.clone())
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

impl Into<Planner> for SteamDeck {
    fn into(self) -> Planner {
        Planner::SteamDeck
    }
}
