use crate::{planner::Plannable, Planner};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SteamDeck;

#[async_trait::async_trait]
impl Plannable for SteamDeck {
    const DISPLAY_STRING: &'static str = "Steam Deck (x86_64 Linux Multi-User)";
    const SLUG: &'static str = "steam-deck";

    async fn plan(
        settings: crate::InstallSettings,
    ) -> Result<crate::InstallPlan, crate::planner::PlannerError> {
        todo!()
    }
}

impl Into<Planner> for SteamDeck {
    fn into(self) -> Planner {
        Planner::SteamDeck
    }
}
