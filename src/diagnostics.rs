use os_release::OsRelease;
use reqwest::Url;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Copy)]
pub enum DiagnosticStatus {
    Cancelled,
    Success,
    Failure,
    Pending,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Copy)]
pub enum DiagnosticAction {
    Install,
    Uninstall,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct DiagnosticReport {
    version: String,
    planner: String,
    configured_settings: Vec<String>,
    os_name: String,
    os_version: String,
    architecture: String,
    action: DiagnosticAction,
    status: DiagnosticStatus,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct DiagnosticData {
    version: String,
    planner: String,
    configured_settings: Vec<String>,
    os_name: String,
    os_version: String,
    architecture: String,
    endpoint: Option<Url>,
}

impl DiagnosticData {
    pub fn new(endpoint: Option<Url>, planner: String, configured_settings: Vec<String>) -> Self {
        let (os_name, os_version) = match OsRelease::new() {
            Ok(os_release) => (os_release.name, os_release.version),
            Err(_) => ("unknown".into(), "unknown".into()),
        };
        Self {
            endpoint,
            version: env!("CARGO_PKG_VERSION").into(),
            planner,
            configured_settings,
            os_name,
            os_version,
            architecture: std::env::consts::ARCH.to_string(),
        }
    }

    pub fn report(&self, action: DiagnosticAction, status: DiagnosticStatus) -> DiagnosticReport {
        let Self {
            version,
            planner,
            configured_settings,
            os_name,
            os_version,
            architecture,
            endpoint: _,
        } = self;
        DiagnosticReport {
            version: version.clone(),
            planner: planner.clone(),
            configured_settings: configured_settings.clone(),
            os_name: os_name.clone(),
            os_version: os_version.clone(),
            architecture: architecture.clone(),
            action,
            status,
        }
    }

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
                let client = reqwest::Client::new();
                let res = client
                    .post(endpoint.clone())
                    .body(serialized)
                    .header("Content-Type", "application/json")
                    .send()
                    .await;

                if let Err(err) = res {
                    tracing::info!(?err, "Failed to send diagnostic to endpoint, continuing")
                }
            },
            "file" => {
                let res = tokio::fs::write(endpoint.path(), serialized).await;

                if let Err(err) = res {
                    tracing::info!(?err, "Failed to send diagnostic to endpoint, continuing")
                }
            },
            _ => return Err(DiagnosticError::UnknownUrlScheme),
        };
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
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
