use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};

/**
Bootstrap and kickstart an APFS volume
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct BootstrapLaunchctlService {
    domain: String,
    service: String,
    path: PathBuf,
}

impl BootstrapLaunchctlService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        domain: impl AsRef<str>,
        service: impl AsRef<str>,
        path: impl AsRef<Path>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let domain = domain.as_ref().to_string();
        let service = service.as_ref().to_string();
        let path = path.as_ref().to_path_buf();

        let output = Command::new("launchctl")
            .process_group(0)
            .arg("print")
            .arg(format!("{domain}/{service}"))
            .arg("-plist")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .map_err(|e| ActionError::Command(e))?;
        if output.status.success() || output.status.code() == Some(37) {
            // We presume that success means it's found
            return Ok(StatefulAction::completed(Self {
                service,
                domain,
                path,
            }));
        }

        Ok(StatefulAction::uncompleted(Self {
            domain,
            service,
            path,
        }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "bootstrap_launchctl_service")]
impl Action for BootstrapLaunchctlService {
    fn tracing_synopsis(&self) -> String {
        format!(
            "Bootstrap the `{}` service via `launchctl bootstrap {} {}`",
            self.service,
            self.domain,
            self.path.display()
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "bootstrap_launchctl_service",
            domain = self.domain,
            path = %self.path.display(),
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            domain,
            service: _,
            path,
        } = self;

        execute_command(
            Command::new("launchctl")
                .process_group(0)
                .arg("bootstrap")
                .arg(domain)
                .arg(path)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| ActionError::Command(e))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!(
                "Run `launchctl bootout {} {}`",
                self.domain,
                self.path.display()
            ),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            service: _,
            domain,
        } = self;

        execute_command(
            Command::new("launchctl")
                .process_group(0)
                .arg("bootout")
                .arg(domain)
                .arg(path)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| ActionError::Command(e))?;

        Ok(())
    }
}
