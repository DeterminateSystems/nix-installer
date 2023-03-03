/*! Diagnostic reporting functionality

When enabled with the `diagnostics` feature (default) this module provides automated install success/failure reporting to an endpoint.

That endpoint can be a URL such as `https://our.project.org/nix-installer/diagnostics` or `file:///home/$USER/diagnostic.json` which receives a [`DiagnosticReport`] in JSON format.
*/

use std::time::Duration;

use os_release::OsRelease;
use reqwest::Url;

use crate::{
    action::ActionError, planner::PlannerError, settings::InstallSettingsError, NixInstallerError,
};

/// The static of an action attempt
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub enum DiagnosticStatus {
    Cancelled,
    Success,
    Pending,
    Failure,
}

/// The action attempted
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Copy)]
pub enum DiagnosticAction {
    Install,
    Uninstall,
}

/// A report sent to an endpoint
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct DiagnosticReport {
    pub version: String,
    pub planner: String,
    pub configured_settings: Vec<String>,
    pub os_name: String,
    pub os_version: String,
    pub triple: String,
    pub is_ci: bool,
    pub action: DiagnosticAction,
    pub status: DiagnosticStatus,
    /// Generally this includes the [`strum::IntoStaticStr`] representation of the error, we take special care not to include parameters of the error (which may include secrets)
    pub failure_chain: Option<Vec<String>>,
}

/// A preparation of data to be sent to the `endpoint`.
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct DiagnosticData {
    version: String,
    planner: String,
    configured_settings: Vec<String>,
    os_name: String,
    os_version: String,
    triple: String,
    is_ci: bool,
    endpoint: Option<Url>,
    /// Generally this includes the [`strum::IntoStaticStr`] representation of the error, we take special care not to include parameters of the error (which may include secrets)
    failure_chain: Option<Vec<String>>,
}

impl DiagnosticData {
    pub fn new(endpoint: Option<Url>, planner: String, configured_settings: Vec<String>) -> Self {
        let (os_name, os_version) = match OsRelease::new() {
            Ok(os_release) => (os_release.name, os_release.version),
            Err(_) => ("unknown".into(), "unknown".into()),
        };
        let is_ci = is_ci::cached()
            || std::env::var("NIX_INSTALLER_CI").unwrap_or_else(|_| "0".into()) == "1";
        Self {
            endpoint,
            version: env!("CARGO_PKG_VERSION").into(),
            planner,
            configured_settings,
            os_name,
            os_version,
            triple: target_lexicon::HOST.to_string(),
            is_ci,
            failure_chain: None,
        }
    }

    pub fn failure(mut self, err: &NixInstallerError) -> Self {
        let mut failure_chain = vec![];
        let diagnostic = err.diagnostic();
        failure_chain.push(diagnostic);

        let mut walker: &dyn std::error::Error = &err;
        while let Some(source) = walker.source() {
            if let Some(downcasted) = source.downcast_ref::<ActionError>() {
                let downcasted_diagnostic = downcasted.diagnostic();
                failure_chain.push(downcasted_diagnostic);
            }
            if let Some(downcasted) = source.downcast_ref::<Box<ActionError>>() {
                let downcasted_diagnostic = downcasted.diagnostic();
                failure_chain.push(downcasted_diagnostic);
            }
            if let Some(downcasted) = source.downcast_ref::<PlannerError>() {
                let downcasted_diagnostic = downcasted.diagnostic();
                failure_chain.push(downcasted_diagnostic);
            }
            if let Some(downcasted) = source.downcast_ref::<InstallSettingsError>() {
                let downcasted_diagnostic = downcasted.diagnostic();
                failure_chain.push(downcasted_diagnostic);
            }
            if let Some(downcasted) = source.downcast_ref::<DiagnosticError>() {
                let downcasted_diagnostic = downcasted.diagnostic();
                failure_chain.push(downcasted_diagnostic);
            }

            walker = source;
        }

        self.failure_chain = Some(failure_chain);
        self
    }

    pub fn report(&self, action: DiagnosticAction, status: DiagnosticStatus) -> DiagnosticReport {
        let Self {
            version,
            planner,
            configured_settings,
            os_name,
            os_version,
            triple,
            is_ci,
            endpoint: _,
            failure_chain,
        } = self;
        DiagnosticReport {
            version: version.clone(),
            planner: planner.clone(),
            configured_settings: configured_settings.clone(),
            os_name: os_name.clone(),
            os_version: os_version.clone(),
            triple: triple.clone(),
            is_ci: *is_ci,
            action,
            status,
            failure_chain: failure_chain.clone(),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn send(
        self,
        action: DiagnosticAction,
        status: DiagnosticStatus,
    ) -> Result<(), DiagnosticError> {
        let serialized = serde_json::to_string_pretty(&self.report(action, status))?;

        let endpoint = match self.endpoint {
            Some(endpoint) => endpoint,
            None => return Ok(()),
        };

        match endpoint.scheme() {
            "https" | "http" => {
                tracing::debug!("Sending diagnostic to `{endpoint}`");
                let client = reqwest::Client::new();
                let res = client
                    .post(endpoint.clone())
                    .body(serialized)
                    .header("Content-Type", "application/json")
                    .timeout(Duration::from_millis(3000))
                    .send()
                    .await;

                if let Err(_err) = res {
                    tracing::info!("Failed to send diagnostic to `{endpoint}`, continuing")
                }
            },
            "file" => {
                let path = endpoint.path();
                tracing::debug!("Writing diagnostic to `{path}`");
                let res = tokio::fs::write(path, serialized).await;

                if let Err(_err) = res {
                    tracing::info!("Failed to send diagnostic to `{path}`, continuing")
                }
            },
            _ => return Err(DiagnosticError::UnknownUrlScheme),
        };
        Ok(())
    }
}

#[derive(thiserror::Error, Debug, strum::IntoStaticStr)]
pub enum DiagnosticError {
    #[error("Unknown url scheme")]
    UnknownUrlScheme,
    #[error("Request error")]
    Reqwest(
        #[from]
        #[source]
        reqwest::Error,
    ),
    #[error("Write path `{0}`")]
    Write(std::path::PathBuf, #[source] std::io::Error),
    #[error("Serializing receipt")]
    Serializing(
        #[from]
        #[source]
        serde_json::Error,
    ),
}

pub trait ErrorDiagnostic {
    fn diagnostic(&self) -> String;
}

#[cfg(feature = "diagnostics")]
impl ErrorDiagnostic for DiagnosticError {
    fn diagnostic(&self) -> String {
        let static_str: &'static str = (self).into();
        return static_str.to_string();
    }
}
