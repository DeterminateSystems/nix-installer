use std::process::Output;

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};

/**
Bootstrap and kickstart an APFS volume
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "kickstart_launchctl_service")]
pub struct KickstartLaunchctlService {
    domain: String,
    service: String,
}

impl KickstartLaunchctlService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        domain: impl AsRef<str>,
        service: impl AsRef<str>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let domain = domain.as_ref().to_string();
        let service = service.as_ref().to_string();

        let mut service_exists = false;
        let mut service_started = false;
        let mut command = Command::new("launchctl");
        command.process_group(0);
        command.arg("print");
        command.arg([domain.as_ref(), service.as_ref()].join("/"));
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        let output = command
            .output()
            .await
            .map_err(|e| Self::error(ActionErrorKind::command(&command, e)))?;
        if output.status.success() {
            service_exists = true;

            let output_string = String::from_utf8(output.stdout).map_err(Self::error)?;
            // We are looking for a line containing "state = " with some trailing content
            // The output is not a JSON or a plist
            // MacOS's man pages explicitly tell us not to try to parse this output
            // MacOS's man pages explicitly tell us this output is not stable
            // Yet, here we are, doing exactly that.
            for output_line in output_string.lines() {
                let output_line_trimmed = output_line.trim();
                if output_line_trimmed.starts_with("state") {
                    if output_line_trimmed.contains("running") {
                        service_started = true;
                    }
                    break;
                }
            }
        }

        if service_exists && service_started {
            return Ok(StatefulAction::completed(Self { domain, service }));
        }

        // It's safe to assume the user does not have the service started
        Ok(StatefulAction::uncompleted(Self { domain, service }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "kickstart_launchctl_service")]
impl Action for KickstartLaunchctlService {
    fn action_tag() -> ActionTag {
        ActionTag("kickstart_launchctl_service")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Run `launchctl kickstart -k {}/{}`",
            self.domain, self.service
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "kickstart_launchctl_service",
            path = %self.service,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { domain, service } = self;

        execute_command(
            Command::new("launchctl")
                .process_group(0)
                .args(["kickstart", "-k"])
                .arg(format!("{domain}/{service}"))
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Run `launchctl stop {}`", self.service),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        // MacOs doesn't offer an "ensure-stopped" like they do with Kickstart
        let mut command = Command::new("launchctl");
        command.process_group(0);
        command.arg("stop");
        command.arg(format!("{}/{}", self.domain, self.service));
        command.stdin(std::process::Stdio::null());
        let command_str = format!("{:?}", command.as_std());

        let output = command
            .output()
            .await
            .map_err(|e| Self::error(ActionErrorKind::command(&command, e)))?;

        // On our test Macs, a status code of `3` was reported if the service was stopped while not running.
        match output.status.code() {
            Some(3) | Some(0) | None => (),
            _ => {
                return Err(Self::error(ActionErrorKind::Custom(Box::new(
                    KickstartLaunchctlServiceError::CannotStopService(command_str, output),
                ))))
            },
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KickstartLaunchctlServiceError {
    #[error("Command `{0}` failed, stderr: {}", String::from_utf8(.1.stderr.clone()).unwrap_or_else(|_e| String::from("<Non-UTF-8>")))]
    CannotStopService(String, Output),
}
