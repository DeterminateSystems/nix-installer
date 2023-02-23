/*! [`BuiltinPlanner`]s and traits to create new types which can be used to plan out an [`InstallPlan`]

It's a [`Planner`]s job to construct (if possible) a valid [`InstallPlan`] for the host. Some planners are operating system specific, others are device specific.

[`Planner`]s contain their planner specific settings, typically alongside a [`CommonSettings`][crate::settings::CommonSettings].

[`BuiltinPlanner::default()`] offers a way to get the default builtin planner for a given host.

Custom Planners can also be used to create a platform, project, or organization specific install.

A custom [`Planner`] can be created:

```rust,no_run
use std::{error::Error, collections::HashMap};
use nix_installer::{
    InstallPlan,
    settings::{CommonSettings, InstallSettingsError},
    planner::{Planner, PlannerError},
    action::{Action, StatefulAction, base::CreateFile},
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
            common: CommonSettings::default().await?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        Ok(vec![
            // ...

                CreateFile::plan("/example", None, None, None, "Example".to_string(), false)
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

    #[cfg(feature = "diagnostics")]
    async fn diagnostic_data(&self) -> Result<nix_installer::diagnostics::DiagnosticData, PlannerError> {
        Ok(nix_installer::diagnostics::DiagnosticData::new(
            self.common.diagnostic_endpoint.clone(),
            self.typetag_name().into(),
            self.configured_settings().await?,
        ))
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
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub mod steam_deck;

use std::{collections::HashMap, string::FromUtf8Error};

use crate::{
    action::{ActionError, StatefulAction},
    error::HasExpectedErrors,
    settings::{CommonSettings, InstallSettingsError},
    Action, InstallPlan, NixInstallerError,
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

    async fn configured_settings(&self) -> Result<Vec<String>, PlannerError>
    where
        Self: Sized,
    {
        let default = Self::default().await?.settings()?;
        let configured = self.settings()?;

        let mut keys: Vec<String> = Vec::new();
        for (key, value) in configured.iter() {
            if default.get(key) != Some(value) {
                keys.push(key.clone())
            }
        }
        Ok(keys)
    }

    /// A boxed, type erased planner
    fn boxed(self) -> Box<dyn Planner>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }

    #[cfg(feature = "diagnostics")]
    async fn diagnostic_data(&self) -> Result<crate::diagnostics::DiagnosticData, PlannerError>;
}

dyn_clone::clone_trait_object!(Planner);

/// Planners built into this crate
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Subcommand))]
pub enum BuiltinPlanner {
    #[cfg(target_os = "linux")]
    /// A planner for Linux installs
    Linux(linux::Linux),
    /// A planner MacOS (Darwin) for installs
    #[cfg(target_os = "macos")]
    Macos(macos::Macos),
    /// A planner suitable for the Valve Steam Deck running SteamOS
    #[cfg(target_os = "linux")]
    SteamDeck(steam_deck::SteamDeck),
}

impl BuiltinPlanner {
    /// Heuristically determine the default planner for the target system
    pub async fn default() -> Result<Self, PlannerError> {
        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            #[cfg(target_os = "linux")]
            (Architecture::X86_64, OperatingSystem::Linux) => {
                Ok(Self::Linux(linux::Linux::default().await?))
            },
            #[cfg(target_os = "linux")]
            (Architecture::X86_32(_), OperatingSystem::Linux) => {
                Ok(Self::Linux(linux::Linux::default().await?))
            },
            #[cfg(target_os = "linux")]
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                Ok(Self::Linux(linux::Linux::default().await?))
            },
            #[cfg(target_os = "macos")]
            (Architecture::X86_64, OperatingSystem::MacOSX { .. })
            | (Architecture::X86_64, OperatingSystem::Darwin) => {
                Ok(Self::Macos(macos::Macos::default().await?))
            },
            #[cfg(target_os = "macos")]
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. })
            | (Architecture::Aarch64(_), OperatingSystem::Darwin) => {
                Ok(Self::Macos(macos::Macos::default().await?))
            },
            _ => Err(PlannerError::UnsupportedArchitecture(target_lexicon::HOST)),
        }
    }

    pub async fn from_common_settings(settings: CommonSettings) -> Result<Self, PlannerError> {
        let mut built = Self::default().await?;
        match &mut built {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(inner) => inner.settings = settings,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(inner) => inner.settings = settings,
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(inner) => inner.settings = settings,
        }
        Ok(built)
    }

    pub async fn configured_settings(&self) -> Result<Vec<String>, PlannerError> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(inner) => inner.configured_settings().await,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(inner) => inner.configured_settings().await,
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(inner) => inner.configured_settings().await,
        }
    }

    pub async fn plan(self) -> Result<InstallPlan, NixInstallerError> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(planner) => InstallPlan::plan(planner).await,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(planner) => InstallPlan::plan(planner).await,
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(planner) => InstallPlan::plan(planner).await,
        }
    }
    pub fn boxed(self) -> Box<dyn Planner> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(i) => i.boxed(),
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(i) => i.boxed(),
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(i) => i.boxed(),
        }
    }

    pub fn typetag_name(&self) -> &'static str {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(i) => i.typetag_name(),
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(i) => i.typetag_name(),
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(i) => i.typetag_name(),
        }
    }

    pub fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(i) => i.settings(),
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(i) => i.settings(),
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(i) => i.settings(),
        }
    }

    #[cfg(feature = "diagnostics")]
    pub async fn diagnostic_data(
        &self,
    ) -> Result<crate::diagnostics::DiagnosticData, PlannerError> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(i) => i.diagnostic_data().await,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(i) => i.diagnostic_data().await,
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(i) => i.diagnostic_data().await,
        }
    }
}

/// An error originating from a [`Planner`]
#[derive(thiserror::Error, Debug, strum::IntoStaticStr)]
pub enum PlannerError {
    /// `nix-installer` does not have a default planner for the target architecture right now
    #[error("`nix-installer` does not have a default planner for the `{0}` architecture right now, pass a specific archetype")]
    UnsupportedArchitecture(target_lexicon::Triple),
    /// Error executing action
    #[error("Error executing action")]
    Action(
        #[source]
        #[from]
        ActionError,
    ),
    /// An [`InstallSettingsError`]
    #[error(transparent)]
    InstallSettings(#[from] InstallSettingsError),
    /// A MacOS (Darwin) plist related error
    #[error(transparent)]
    Plist(#[from] plist::Error),
    /// A Linux SELinux related error
    #[error("This installer doesn't yet support SELinux in `Enforcing` mode. If SELinux is important to you, please see https://github.com/DeterminateSystems/nix-installer/issues/124. You can also try again after setting SELinux to `Permissive` mode with `setenforce Permissive`")]
    SelinuxEnforcing,
    /// A UTF-8 related error
    #[error("UTF-8 error")]
    Utf8(#[from] FromUtf8Error),
    /// Custom planner error
    #[error("Custom planner error")]
    Custom(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("NixOS already has Nix installed")]
    NixOs,
    #[error("`nix` is already a valid command, so it is installed")]
    NixExists,
}

impl HasExpectedErrors for PlannerError {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            this @ PlannerError::UnsupportedArchitecture(_) => Some(Box::new(this)),
            PlannerError::Action(_) => None,
            PlannerError::InstallSettings(_) => None,
            PlannerError::Plist(_) => None,
            PlannerError::Utf8(_) => None,
            PlannerError::SelinuxEnforcing => Some(Box::new(self)),
            PlannerError::Custom(_) => None,
            this @ PlannerError::NixOs => Some(Box::new(this)),
            this @ PlannerError::NixExists => Some(Box::new(this)),
        }
    }
}
