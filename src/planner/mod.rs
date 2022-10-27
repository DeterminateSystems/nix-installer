pub mod darwin;
pub mod linux;
pub mod specific;

use crate::{settings::InstallSettingsError, BoxableError, InstallPlan};

#[async_trait::async_trait]
#[typetag::serde(tag = "planner")]
pub trait Planner: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    async fn default() -> Result<Self, Box<dyn std::error::Error + Sync + Send>>
    where
        Self: Sized;
    async fn plan(self) -> Result<InstallPlan, Box<dyn std::error::Error + Sync + Send>>;
}

dyn_clone::clone_trait_object!(Planner);

#[derive(Debug, Clone, clap::Subcommand, serde::Serialize, serde::Deserialize)]
pub enum BuiltinPlanner {
    LinuxMulti(linux::LinuxMulti),
    DarwinMulti(darwin::DarwinMulti),
    SteamDeck(specific::SteamDeck),
}

impl BuiltinPlanner {
    pub async fn default() -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
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
            _ => Err(BuiltinPlannerError::UnsupportedArchitecture(target_lexicon::HOST).boxed()),
        }
    }

    pub async fn plan(self) -> Result<InstallPlan, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            BuiltinPlanner::LinuxMulti(planner) => planner.plan().await,
            BuiltinPlanner::DarwinMulti(planner) => planner.plan().await,
            BuiltinPlanner::SteamDeck(planner) => planner.plan().await,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum BuiltinPlannerError {
    #[error("Harmonic does not have a default planner for the `{0}` architecture right now, pass a specific archetype")]
    UnsupportedArchitecture(target_lexicon::Triple),
    #[error("Error executing action")]
    ActionError(
        #[source]
        #[from]
        Box<dyn std::error::Error + Send + Sync>,
    ),
    #[error(transparent)]
    InstallSettings(#[from] InstallSettingsError),
    #[error(transparent)]
    Plist(#[from] plist::Error),
}
