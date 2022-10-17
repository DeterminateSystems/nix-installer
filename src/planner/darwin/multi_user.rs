use crate::{
    actions::{
        meta::{darwin::CreateApfsVolume, ConfigureNix, ProvisionNix, StartNixDaemon},
        Action, ActionError,
    },
    planner::Plannable,
    InstallPlan, Planner,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct DarwinMultiUser;

#[async_trait::async_trait]
impl Plannable for DarwinMultiUser {
    const DISPLAY_STRING: &'static str = "Darwin Multi-User";
    const SLUG: &'static str = "darwin-multi";

    async fn plan(
        settings: crate::InstallSettings,
    ) -> Result<crate::InstallPlan, crate::planner::PlannerError> {
        Ok(InstallPlan {
            planner: Self.into(),
            settings: settings.clone(),
            actions: vec![
                // Create Volume step:
                //
                // setup_Synthetic -> create_synthetic_objects
                // Unmount -> create_volume -> Setup_fstab -> maybe encrypt_volume -> launchctl bootstrap -> launchctl kickstart -> await_volume -> maybe enableOwnership
                CreateApfsVolume::plan(settings.clone())
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ProvisionNix::plan(settings.clone())
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ConfigureNix::plan(settings)
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                StartNixDaemon::plan()
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
            ],
        })
    }
}

impl Into<Planner> for DarwinMultiUser {
    fn into(self) -> Planner {
        Planner::DarwinMultiUser
    }
}
