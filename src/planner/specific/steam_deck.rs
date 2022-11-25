use std::collections::HashMap;

use crate::{
    action::{
        base::CreateDirectory,
        common::ProvisionNix,
        linux::{CreateSystemdSysext, StartSystemdUnit},
        StatefulAction,
    },
    planner::{Planner, PlannerError},
    settings::CommonSettings,
    settings::InstallSettingsError,
    Action, BuiltinPlanner,
};

/// A planner suitable for Valve Steam Deck consoles
#[derive(Debug, Clone, clap::Parser, serde::Serialize, serde::Deserialize)]
pub struct SteamDeck {
    #[clap(flatten)]
    pub settings: CommonSettings,
}

#[async_trait::async_trait]
#[typetag::serde(name = "steam-deck")]
impl Planner for SteamDeck {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: CommonSettings::default()?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        Ok(vec![
            CreateSystemdSysext::plan("/var/lib/extensions/nix")
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            CreateDirectory::plan("/nix", None, None, 0o0755, true)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            ProvisionNix::plan(&self.settings.clone())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
            StartSystemdUnit::plan("nix-daemon.socket".into())
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        ])
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self { settings } = self;
        let mut map = HashMap::default();

        map.extend(settings.settings()?.into_iter());

        Ok(map)
    }
}

impl Into<BuiltinPlanner> for SteamDeck {
    fn into(self) -> BuiltinPlanner {
        BuiltinPlanner::SteamDeck(self)
    }
}
