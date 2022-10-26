use crate::{
    actions::{
        base::{CreateDirectory, StartSystemdUnit},
        meta::{ConfigureNix, ProvisionNix},
        Action, ActionError,
    },
    planner::{Plannable, PlannerError},
    BuiltinPlanner, CommonSettings, InstallPlan,
};

#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct LinuxMulti {
    #[clap(flatten)]
    settings: CommonSettings,
}

#[async_trait::async_trait]
impl Plannable for LinuxMulti {
    const DISPLAY_STRING: &'static str = "Linux Multi-User";
    const SLUG: &'static str = "linux-multi";

    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(self) -> Result<InstallPlan, PlannerError> {
        Ok(InstallPlan {
            planner: self.clone().into(),
            actions: vec![
                CreateDirectory::plan("/nix", None, None, 0o0755, true)
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ProvisionNix::plan(self.settings.clone())
                    .await
                    .map(Action::from)
                    .map_err(ActionError::from)?,
                ConfigureNix::plan(self.settings)
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

impl Into<BuiltinPlanner> for LinuxMulti {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::LinuxMulti(self)
    }
}
