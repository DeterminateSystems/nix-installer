mod darwin;
mod linux;
mod specific;

use crate::{actions::ActionError, InstallPlan, InstallSettings};

#[derive(Debug, Clone, clap::ValueEnum, serde::Serialize, serde::Deserialize)]
pub enum Planner {
    LinuxMultiUser,
    DarwinMultiUser,
    SteamDeck,
}

impl Planner {
    pub fn possible_values() -> &'static [Planner] {
        &[Self::LinuxMultiUser, Self::DarwinMultiUser, Self::SteamDeck]
    }
    pub fn default() -> Result<Self, PlannerError> {
        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            (Architecture::X86_64, OperatingSystem::Linux) => Ok(Self::LinuxMultiUser),
            (Architecture::Aarch64(_), OperatingSystem::Linux) => Ok(Self::LinuxMultiUser),
            (Architecture::X86_64, OperatingSystem::MacOSX { .. }) => Ok(Self::DarwinMultiUser),
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. }) => Ok(Self::DarwinMultiUser),
            _ => Err(PlannerError::UnsupportedArchitecture(target_lexicon::HOST)),
        }
    }

    pub async fn plan(self, settings: InstallSettings) -> Result<InstallPlan, PlannerError> {
        match self {
            Planner::LinuxMultiUser => linux::LinuxMultiUser::plan(settings).await,
            Planner::DarwinMultiUser => darwin::DarwinMultiUser::plan(settings).await,
            Planner::SteamDeck => specific::SteamDeck::plan(settings).await,
        }
    }
}

#[async_trait::async_trait]
trait Plannable: Into<Planner>
where
    Self: Sized,
{
    const DISPLAY_STRING: &'static str;
    const SLUG: &'static str;

    async fn plan(settings: InstallSettings) -> Result<InstallPlan, PlannerError>;
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
}
