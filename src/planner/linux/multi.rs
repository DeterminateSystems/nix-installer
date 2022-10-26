use crate::{
    actions::{
        base::{CreateDirectory, StartSystemdUnit},
        meta::{ConfigureNix, ProvisionNix},
    },
    planner::{BuiltinPlannerError, Plannable},
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
    type Error = BuiltinPlannerError;

    async fn default() -> Result<Self, Self::Error> {
        Ok(Self {
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(self) -> Result<InstallPlan, Self::Error> {
        Ok(InstallPlan {
            planner: self.clone().into(),
            actions: vec![
                Box::new(CreateDirectory::plan("/nix", None, None, 0o0755, true).await?),
                Box::new(ProvisionNix::plan(self.settings.clone()).await?),
                Box::new(ConfigureNix::plan(self.settings).await?),
                Box::new(StartSystemdUnit::plan("nix-daemon.socket".into()).await?),
            ],
        })
    }
}

impl Into<BuiltinPlanner> for LinuxMulti {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::LinuxMulti(self)
    }
}
