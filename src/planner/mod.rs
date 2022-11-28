/*! [`BuiltinPlanner`]s and traits to create new types which can be used to plan out an [`InstallPlan`]

It's a [`Planner`]s job to construct (if possible) a valid [`InstallPlan`] for the host. Some planners,
like [`LinuxMulti`](linux::LinuxMulti), are operating system specific. Others, like [`SteamDeck`](linux::SteamDeck), are device specific.

[`Planner`]s contain their planner specific settings, typically alongside a [`CommonSettings`][crate::settings::CommonSettings].

[`BuiltinPlanner::default()`] offers a way to get the default builtin planner for a given host.

Custom Planners can also be used to create a platform, project, or organization specific install.

A custom [`Planner`] can be created:

```rust,no_run
use std::{error::Error, collections::HashMap};
use harmonic::{
    InstallPlan,
    settings::{CommonSettings, InstallSettingsError},
    planner::{Planner, PlannerError, specific::SteamDeck},
    action::{Action, StatefulAction, linux::StartSystemdUnit},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MyPlanner {
    pub common: CommonSettings,
}


#[async_trait::async_trait]
#[typetag::serde(name = "my-planner")]
impl Planner for MyPlanner {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            common: CommonSettings::default()?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        Ok(vec![
            // ...

                StartSystemdUnit::plan("nix-daemon.socket".into())
                    .await
                    .map_err(PlannerError::Action)?.boxed(),
        ])
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self { common } = self;
        let mut map = std::collections::HashMap::default();

        map.extend(common.settings()?.into_iter());

        Ok(map)
    }
}

# async fn custom_planner_install() -> color_eyre::Result<()> {
let planner = MyPlanner::default().await?;
let mut plan = InstallPlan::plan(planner).await?;
match plan.install(None).await {
    Ok(()) => tracing::info!("Done"),
    Err(e) => {
        match e.source() {
            Some(source) => tracing::error!("{e}: {}", source),
            None => tracing::error!("{e}"),
        };
        plan.uninstall(None).await?;
    },
};

#    Ok(())
# }
```

*/
pub mod darwin;
pub mod linux;

use std::collections::HashMap;

use crate::{
    action::StatefulAction, settings::InstallSettingsError, Action, HarmonicError, InstallPlan,
};

/// Something which can be used to plan out an [`InstallPlan`]
#[async_trait::async_trait]
#[typetag::serde(tag = "planner")]
pub trait Planner: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    /// Instantiate the planner with default settings, if possible
    async fn default() -> Result<Self, PlannerError>
    where
        Self: Sized;
    /// Plan out the [`Action`]s for an [`InstallPlan`]
    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError>;
    /// The settings being used by the planner
    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError>;
    /// A boxed, type erased planner
    fn boxed(self) -> Box<dyn Planner>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

dyn_clone::clone_trait_object!(Planner);

/// Planners built into this crate
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Subcommand))]
pub enum BuiltinPlanner {
    /// A standard Linux multi-user install
    LinuxMulti(linux::LinuxMulti),
    /// A standard MacOS (Darwin) multi-user install
    DarwinMulti(darwin::DarwinMulti),
    SteamDeck(linux::SteamDeck),
}

impl BuiltinPlanner {
    /// Heuristically determine the default planner for the target system
    pub async fn default() -> Result<Self, PlannerError> {
        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            (Architecture::X86_64, OperatingSystem::Linux) => {
                Ok(Self::LinuxMulti(linux::LinuxMulti::default().await?))
            },
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                Ok(Self::LinuxMulti(linux::LinuxMulti::default().await?))
            },
            (Architecture::X86_64, OperatingSystem::MacOSX { .. })
            | (Architecture::X86_64, OperatingSystem::Darwin) => {
                Ok(Self::DarwinMulti(darwin::DarwinMulti::default().await?))
            },
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. })
            | (Architecture::Aarch64(_), OperatingSystem::Darwin) => {
                Ok(Self::DarwinMulti(darwin::DarwinMulti::default().await?))
            },
            _ => Err(PlannerError::UnsupportedArchitecture(target_lexicon::HOST)),
        }
    }

    pub async fn plan(self) -> Result<InstallPlan, HarmonicError> {
        match self {
            BuiltinPlanner::LinuxMulti(planner) => InstallPlan::plan(planner).await,
            BuiltinPlanner::DarwinMulti(planner) => InstallPlan::plan(planner).await,
            BuiltinPlanner::SteamDeck(planner) => InstallPlan::plan(planner).await,
        }
    }
    pub fn boxed(self) -> Box<dyn Planner> {
        match self {
            BuiltinPlanner::LinuxMulti(i) => i.boxed(),
            BuiltinPlanner::DarwinMulti(i) => i.boxed(),
            BuiltinPlanner::SteamDeck(i) => i.boxed(),
        }
    }
}

/// An error originating from a [`Planner`]
#[derive(thiserror::Error, Debug)]
pub enum PlannerError {
    /// Harmonic does not have a default planner for the target architecture right now
    #[error("Harmonic does not have a default planner for the `{0}` architecture right now, pass a specific archetype")]
    UnsupportedArchitecture(target_lexicon::Triple),
    /// Error executing action
    #[error("Error executing action")]
    Action(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// An [`InstallSettingsError`]
    #[error(transparent)]
    InstallSettings(#[from] InstallSettingsError),
    /// A MacOS (Darwin) plist related error
    #[error(transparent)]
    Plist(#[from] plist::Error),
    /// Custom planner error
    #[error("Custom planner error")]
    Custom(#[source] Box<dyn std::error::Error + Send + Sync>),
}
