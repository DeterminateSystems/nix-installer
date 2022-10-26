pub mod darwin;
pub mod linux;
pub mod specific;

use crate::{actions::ActionError, settings::InstallSettingsError, InstallPlan};

#[derive(Debug, Clone, clap::Subcommand, serde::Serialize, serde::Deserialize)]
pub enum BuiltinPlanner {
    LinuxMulti(linux::LinuxMulti),
    DarwinMulti(darwin::DarwinMulti),
    SteamDeck(specific::SteamDeck),
}

impl BuiltinPlanner {
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

    pub async fn plan(self) -> Result<InstallPlan, PlannerError> {
        match self {
            BuiltinPlanner::LinuxMulti(planner) => planner.plan().await,
            BuiltinPlanner::DarwinMulti(planner) => planner.plan().await,
            BuiltinPlanner::SteamDeck(planner) => planner.plan().await,
        }
    }
}

#[async_trait::async_trait]
trait Plannable: Into<BuiltinPlanner>
where
    Self: Sized,
{
    const DISPLAY_STRING: &'static str;
    const SLUG: &'static str;

    async fn default() -> Result<Self, PlannerError>;
    async fn plan(self) -> Result<InstallPlan, PlannerError>;
}

#[derive(thiserror::Error, Debug)]
pub enum PlannerError {
    #[error("Harmonic does not have a default planner for the `{0}` architecture right now, pass a specific archetype")]
    UnsupportedArchitecture(target_lexicon::Triple),
    #[error("Error executing action")]
    ActionError(
        #[source]
        #[from]
        ActionError,
    ),
    #[error(transparent)]
    InstallSettings(#[from] InstallSettingsError),
    #[error(transparent)]
    Plist(#[from] plist::Error),
}
