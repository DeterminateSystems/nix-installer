use std::path::PathBuf;

use crate::{action::ActionError, planner::PlannerError, settings::InstallSettingsError};

/// An error occurring during a call defined in this crate
#[derive(thiserror::Error, Debug)]
pub enum NixInstallerError {
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
    #[error("Copying `nix-installer` binary into `/nix`")]
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

    #[cfg(feature = "diagnostics")]
    /// Diagnostics
    #[error("Diagnostic error")]
    Diagnostic(
        #[from]
        #[source]
        crate::diagnostics::DiagnosticError,
    ),
}

pub(crate) trait HasExpectedErrors: std::error::Error + Sized + Send + Sync {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>>;
}

impl HasExpectedErrors for NixInstallerError {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            NixInstallerError::Action(action_error) => action_error.expected(),
            NixInstallerError::RecordingReceipt(_, _) => None,
            NixInstallerError::CopyingSelf(_) => None,
            NixInstallerError::SerializingReceipt(_) => None,
            this @ NixInstallerError::Cancelled => Some(Box::new(this)),
            NixInstallerError::SemVer(_) => None,
            NixInstallerError::Planner(planner_error) => planner_error.expected(),
            NixInstallerError::InstallSettings(_) => None,
            #[cfg(feature = "diagnostics")]
            NixInstallerError::Diagnostic(_) => None,
        }
    }
}
