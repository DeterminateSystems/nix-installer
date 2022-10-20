use crate::{
    actions::{
        base::StartSystemdUnit,
        meta::{ConfigureNix, ProvisionNix},
        Action, ActionError,
    },
    planner::{Plannable, PlannerError},
    InstallPlan, InstallSettings, Planner,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct LinuxMultiUser;

#[async_trait::async_trait]
impl Plannable for LinuxMultiUser {
    const DISPLAY_STRING: &'static str = "Linux Multi-User";
    const SLUG: &'static str = "linux-multi";

    async fn plan(settings: InstallSettings) -> Result<InstallPlan, PlannerError> {
        Ok(InstallPlan {
            planner: Self.into(),
            settings: settings.clone(),
            actions: vec![
                ProvisionNix::plan(settings.clone())
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ConfigureNix::plan(settings)
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

impl Into<Planner> for LinuxMultiUser {
    fn into(self) -> Planner {
        Planner::LinuxMultiUser
    }
}
