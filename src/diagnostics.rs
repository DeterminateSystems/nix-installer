/*! Diagnostic reporting functionality

When enabled with the `diagnostics` feature (default) this module provides automated install success/failure reporting to an endpoint.

That endpoint can be a URL such as `https://our.project.org/nix-installer/diagnostics` or `file:///home/$USER/diagnostic.json` which receives a [`DiagnosticReport`] in JSON format.
*/

use std::path::PathBuf;

use detsys_ids_client::{Builder, Map, Recorder, Worker};
use reqwest::Url;

use crate::{
    action::ActionError, planner::PlannerError, settings::InstallSettingsError, CertificateError,
    NixInstallerError,
};

/// The static of an action attempt
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub enum Status {
    Cancelled,
    Success,
    Pending,
    Failure,
}

/// The action attempted
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Copy)]
pub enum Action {
    Plan,
    Install,
    Uninstall,
    SelfTest,
}

/// A report sent to an endpoint
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct Report {
    action: Action,
    status: Status,
    /// Generally this includes the [`strum::IntoStaticStr`] representation of the error, we take special care not to include parameters of the error (which may include secrets)
    failure_chain: Option<Vec<String>>,
}

impl Report {
    fn new(action: Action, status: Status) -> Self {
        Report {
            action,
            status,
            failure_chain: None,
        }
    }

    fn set_failure_chain(mut self, err: &NixInstallerError) -> Self {
        let mut failure_chain = vec![];
        let diagnostic = err.diagnostic();
        failure_chain.push(diagnostic);

        let mut walker: &dyn std::error::Error = &err;
        while let Some(source) = walker.source() {
            if let Some(downcasted) = source.downcast_ref::<ActionError>() {
                let downcasted_diagnostic = downcasted.kind().diagnostic();
                failure_chain.push(downcasted_diagnostic);
            }
            if let Some(downcasted) = source.downcast_ref::<Box<ActionError>>() {
                let downcasted_diagnostic = downcasted.kind().diagnostic();
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

    fn into_properties(self) -> Option<Map> {
        match serde_json::to_value(&self) {
            Ok(serde_json::Value::Object(m)) => Some(m),
            _ => None,
        }
    }
}

pub async fn diagnostics(
    attribution: Option<String>,
    endpoint: Option<String>,
    ssl_cert_file: Option<std::path::PathBuf>,
    proxy: Option<url::Url>,
) -> (
    crate::feedback::client::Client,
    crate::feedback::client::Worker,
) {
    DiagnosticData::new(attribution, endpoint, ssl_cert_file, proxy)
        .await
        .map(|(c, w)| {
            (
                crate::feedback::client::Client::DiagnosticsData(c),
                crate::feedback::client::Worker::DiagnosticsData(w),
            )
        })
        .unwrap_or_else(|e| {
            tracing::debug!(%e, "Failed to construct the diagnostic data feedback provider.");
            crate::feedback::devnull::dev_null()
        })
}

/// A preparation of data to be sent to the `endpoint`.
#[derive(Clone)]
pub struct DiagnosticData {
    ids_client: Recorder,
}

impl DiagnosticData {
    pub async fn new(
        attribution: Option<String>,
        endpoint: Option<String>,
        ssl_cert_file: Option<PathBuf>,
        proxy: Option<Url>,
    ) -> Result<(Self, Worker), detsys_ids_client::transport::TransportsError> {
        let mut builder: Builder = detsys_ids_client::builder!()
            .set_endpoint(endpoint)
            .set_proxy(proxy);

        if let Some(ssl_cert_file) = ssl_cert_file.and_then(|v| v.canonicalize().ok()) {
            builder = builder.set_certificate(crate::parse_ssl_cert(&ssl_cert_file).await.ok());
        }

        if std::env::var("DETSYS_CORRELATION").ok() != attribution && attribution.is_some() {
            // Don't set the attribution if the attribution was set to the same as DETSYS_CORRELATION
            builder = builder.set_distinct_id(attribution.map(|v| v.into()));
        }
        let (ids_client, ids_worker) = builder.build_or_default().await;

        Ok((Self { ids_client }, ids_worker))
    }

    async fn record(&mut self, report: Report) {
        self.ids_client
            .record("diagnostic", report.into_properties())
            .await;
    }
}

#[non_exhaustive]
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
    /// Parsing URL
    #[error("Parsing URL")]
    Parse(
        #[source]
        #[from]
        url::ParseError,
    ),
    #[error("Write path `{0}`")]
    Write(std::path::PathBuf, #[source] std::io::Error),
    #[error("Serializing receipt")]
    Serializing(
        #[from]
        #[source]
        serde_json::Error,
    ),
    #[error(transparent)]
    Certificate(#[from] CertificateError),
}

pub trait ErrorDiagnostic {
    fn diagnostic(&self) -> String;
}

impl ErrorDiagnostic for DiagnosticError {
    fn diagnostic(&self) -> String {
        let static_str: &'static str = (self).into();
        static_str.to_string()
    }
}

impl crate::feedback::Feedback for DiagnosticData {
    async fn get_feature_ptr_payload<T: serde::de::DeserializeOwned + Send + std::fmt::Debug>(
        &mut self,
        name: impl Into<String> + std::fmt::Debug + Send,
    ) -> Option<T> {
        self.ids_client.get_feature_ptr_payload::<T>(name).await
    }

    async fn set_planner(
        &mut self,
        planner: &crate::planner::BuiltinPlanner,
    ) -> Result<(), crate::planner::PlannerError> {
        self.ids_client
            .add_fact("planner", planner.typetag_name().into())
            .await;

        if let Ok(ref settings) = planner.configured_settings().await {
            self.ids_client
                .add_fact(
                    "configured_settings",
                    settings.keys().cloned().collect::<Vec<_>>().into(),
                )
                .await;

            self.ids_client
                .add_fact(
                    "install_determinate_nix",
                    settings
                        .get("determinate_nix")
                        .cloned()
                        .unwrap_or(serde_json::Value::Bool(false)),
                )
                .await;
        }

        Ok(())
    }

    async fn planning_failed(&mut self, error: &crate::error::NixInstallerError) {
        self.record(Report::new(Action::Plan, Status::Failure).set_failure_chain(error))
            .await;
    }

    async fn planning_succeeded(&mut self) {
        self.record(Report::new(Action::Plan, Status::Success))
            .await;
    }

    async fn install_cancelled(&mut self) {
        self.record(Report::new(Action::Install, Status::Cancelled))
            .await;
    }

    async fn install_failed(&mut self, error: &crate::error::NixInstallerError) {
        self.record(Report::new(Action::Install, Status::Failure).set_failure_chain(error))
            .await;
    }

    async fn self_test_failed(&mut self, error: &crate::error::NixInstallerError) {
        self.ids_client
            .record(
                "nix-installer:self-test-failure",
                Report::new(Action::SelfTest, Status::Failure)
                    .set_failure_chain(error)
                    .into_properties(),
            )
            .await
    }

    async fn install_succeeded(&mut self) {
        self.record(Report::new(Action::Install, Status::Success))
            .await;
    }

    async fn uninstall_cancelled(&mut self) {
        self.record(Report::new(Action::Uninstall, Status::Cancelled))
            .await;
    }

    async fn uninstall_failed(&mut self, error: &crate::error::NixInstallerError) {
        self.record(Report::new(Action::Uninstall, Status::Failure).set_failure_chain(error))
            .await;
    }

    async fn uninstall_succeeded(&mut self) {
        self.record(Report::new(Action::Uninstall, Status::Success))
            .await;
    }
}

impl crate::feedback::FeedbackWorker for Worker {
    async fn submit(self) {
        self.wait().await;
    }
}
