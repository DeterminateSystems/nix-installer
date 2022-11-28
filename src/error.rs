use std::path::PathBuf;

use crate::{planner::PlannerError, settings::InstallSettingsError};

/// An error occurring during a call defined in this crate
#[derive(thiserror::Error, Debug)]
pub enum HarmonicError {
    /// An error originating from an [`Action`](crate::action::Action)
    #[error("Error executing action")]
    Action(
        #[source]
        #[from]
        Box<dyn std::error::Error + Send + Sync>,
    ),
    /// An error while writing the [`InstallPlan`](crate::InstallPlan)
    #[error("Recording install receipt")]
    RecordingReceipt(PathBuf, #[source] std::io::Error),
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
