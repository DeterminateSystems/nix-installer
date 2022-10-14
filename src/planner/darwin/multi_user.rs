use crate::{planner::Plannable, Planner};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct DarwinMultiUser;

#[async_trait::async_trait]
impl Plannable for DarwinMultiUser {
    const DISPLAY_STRING: &'static str = "Darwin Multi-User";
    const SLUG: &'static str = "darwin-multi";

    async fn plan(
        settings: crate::InstallSettings,
    ) -> Result<crate::InstallPlan, crate::planner::PlannerError> {
        todo!()
    }
}

impl Into<Planner> for DarwinMultiUser {
    fn into(self) -> Planner {
        Planner::DarwinMultiUser
    }
}
