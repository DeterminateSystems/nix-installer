use std::path::PathBuf;

use crate::{action::ActionError, planner::PlannerError, settings::InstallSettingsError};

/// An error occurring during a call defined in this crate
#[derive(thiserror::Error, Debug)]
pub enum HarmonicError {
    /// An error originating from an [`Action`](crate::action::Action)
    #[error("Error executing action")]
    Action(
        #[source]
        #[from]
        ActionError,
    ),
    /// An error while writing the [`InstallPlan`](crate::InstallPlan)
    #[error("Recording install receipt")]
    RecordingReceipt(PathBuf, #[source] std::io::Error),
    /// An error while writing copying the binary into the `/nix` folder
    #[error("Copying `harmonic` binary into `/nix`")]
    CopyingSelf(
        #[source]
        #[from]
        std::io::Error,
    ),
    /// An error while serializing the [`InstallPlan`](crate::InstallPlan)
    #[error("Serializing receipt")]
    SerializingReceipt(
        #[from]
        #[source]
        serde_json::Error,
    ),
    /// An error ocurring when a signal is issued along [`InstallPlan::install`](crate::InstallPlan::install)'s `cancel_channel` argument
    #[error("Cancelled by user")]
    Cancelled,
    /// Semver error
    #[error("Semantic Versioning error")]
    SemVer(
        #[from]
        #[source]
        semver::Error,
    ),
    /// Planner error
    #[error("Planner error")]
    Planner(
        #[from]
        #[source]
        PlannerError,
    ),
    /// Install setting error
    #[error("Install setting error")]
    InstallSettings(
        #[from]
        #[source]
        InstallSettingsError,
    ),
}

pub(crate) trait HasExpectedErrors: std::error::Error + Sized + Send + Sync {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>>;
}

impl HasExpectedErrors for HarmonicError {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            HarmonicError::Action(action_error) => action_error.expected(),
            HarmonicError::RecordingReceipt(_, _) => None,
            HarmonicError::CopyingSelf(_) => None,
            HarmonicError::SerializingReceipt(_) => None,
            this @ HarmonicError::Cancelled => Some(Box::new(this)),
            HarmonicError::SemVer(_) => None,
            HarmonicError::Planner(planner_error) => planner_error.expected(),
            HarmonicError::InstallSettings(_) => None,
        }
    }
}
