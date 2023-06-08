use std::{error::Error, path::PathBuf};

use semver::Version;

use crate::{
    action::ActionError, planner::PlannerError, self_test::SelfTestError,
    settings::InstallSettingsError,
};

/// An error occurring during a call defined in this crate
#[non_exhaustive]
#[derive(thiserror::Error, Debug, strum::IntoStaticStr)]
pub enum NixInstallerError {
    /// An error originating from an [`Action`](crate::action::Action)
    #[error("Error executing action")]
    Action(#[source] ActionError),
    /// An error originating from a [`self_test`](crate::self_test)
    #[error("Self test")]
    SelfTest(
        #[source]
        #[from]
        SelfTestError,
    ),
    /// An error originating from an [`Action`](crate::action::Action) while reverting
    #[error("Error reverting\n{}", .0.iter().map(|err| {
        if let Some(source) = err.source() {
            format!("{err}\n{source}\n")
        } else {
            format!("{err}\n") 
        }
    }).collect::<Vec<_>>().join("\n"))]
    ActionRevert(Vec<ActionError>),
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
    /// An error occurring when a signal is issued along [`InstallPlan::install`](crate::InstallPlan::install)'s `cancel_channel` argument
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
    /// Diagnostic error
    #[error("Diagnostic error")]
    Diagnostic(
        #[from]
        #[source]
        crate::diagnostics::DiagnosticError,
    ),
    /// Could not parse the value as a version requirement in order to ensure it's compatible
    #[error("Could not parse `{0}` as a version requirement in order to ensure it's compatible")]
    InvalidVersionRequirement(String, semver::Error),
    /// Could not parse `nix-installer`'s version as a valid version according to Semantic Versioning, therefore the plan version compatibility cannot be checked
    #[error("Could not parse `nix-installer`'s version `{0}` as a valid version according to Semantic Versioning, therefore the plan version compatibility cannot be checked")]
    InvalidCurrentVersion(String, semver::Error),
    /// This version of `nix-installer` is not compatible with this plan's version
    #[error("`nix-installer` version `{}` is not compatible with this plan's version `{}`", .binary, .plan)]
    IncompatibleVersion { binary: Version, plan: Version },
}

pub(crate) trait HasExpectedErrors: std::error::Error + Sized + Send + Sync {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>>;
}

impl HasExpectedErrors for NixInstallerError {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            NixInstallerError::Action(action_error) => action_error.kind().expected(),
            NixInstallerError::ActionRevert(_) => None,
            NixInstallerError::SelfTest(_) => None,
            NixInstallerError::RecordingReceipt(_, _) => None,
            NixInstallerError::CopyingSelf(_) => None,
            NixInstallerError::SerializingReceipt(_) => None,
            this @ NixInstallerError::Cancelled => Some(Box::new(this)),
            NixInstallerError::SemVer(_) => None,
            NixInstallerError::Planner(planner_error) => planner_error.expected(),
            NixInstallerError::InstallSettings(_) => None,
            this @ NixInstallerError::InvalidVersionRequirement(_, _) => Some(Box::new(this)),
            this @ NixInstallerError::InvalidCurrentVersion(_, _) => Some(Box::new(this)),
            this @ NixInstallerError::IncompatibleVersion { binary: _, plan: _ } => {
                Some(Box::new(this))
            },
            #[cfg(feature = "diagnostics")]
            NixInstallerError::Diagnostic(_) => None,
        }
    }
}

#[cfg(feature = "diagnostics")]
impl crate::diagnostics::ErrorDiagnostic for NixInstallerError {
    fn diagnostic(&self) -> String {
        let static_str: &'static str = (self).into();
        let context = match self {
            Self::SelfTest(self_test) => vec![self_test.diagnostic().to_string()],
            Self::Action(action_error) => vec![action_error.diagnostic().to_string()],
            Self::ActionRevert(action_errors) => action_errors
                .iter()
                .map(|action_error| action_error.diagnostic().to_string())
                .collect(),
            _ => vec![],
        };
        return format!(
            "{}({})",
            static_str,
            context
                .iter()
                .map(|v| format!("\"{v}\""))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}
