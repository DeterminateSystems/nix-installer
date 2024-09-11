use std::path::PathBuf;

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};

use super::{service_is_disabled, DARWIN_LAUNCHD_DOMAIN};

/**
Bootstrap and kickstart an APFS volume
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "bootstrap_launchctl_service")]
pub struct BootstrapLaunchctlService {
    service: String,
    path: PathBuf,
    is_present: bool,
    is_disabled: bool,
}

impl BootstrapLaunchctlService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(service: &str, path: &str) -> Result<StatefulAction<Self>, ActionError> {
        let service = service.to_owned();
        let path = PathBuf::from(path);

        let is_present = {
            let mut command = Command::new("launchctl");
            command.process_group(0);
            command.arg("print");
            command.arg(format!("{DARWIN_LAUNCHD_DOMAIN}/{service}"));
            command.arg("-plist");
            command.stdin(std::process::Stdio::null());
            command.stdout(std::process::Stdio::piped());
            command.stderr(std::process::Stdio::piped());
            let command_output = command
                .output()
                .await
                .map_err(|e| Self::error(ActionErrorKind::command(&command, e)))?;
            // We presume that success means it's found
            command_output.status.success() || command_output.status.code() == Some(37)
        };

        let is_disabled = service_is_disabled(DARWIN_LAUNCHD_DOMAIN, &service)
            .await
            .map_err(Self::error)?;

        if is_present && !is_disabled {
            return Ok(StatefulAction::completed(Self {
                service,
                path,
                is_present,
                is_disabled,
            }));
        }

        Ok(StatefulAction::uncompleted(Self {
            service,
            path,
            is_present,
            is_disabled,
        }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "bootstrap_launchctl_service")]
impl Action for BootstrapLaunchctlService {
    fn action_tag() -> ActionTag {
        ActionTag("bootstrap_launchctl_service")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Bootstrap the `{}` service via `launchctl bootstrap {} {}`",
            self.service,
            DARWIN_LAUNCHD_DOMAIN,
            self.path.display()
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "bootstrap_launchctl_service",
            path = %self.path.display(),
            is_disabled = self.is_disabled,
            is_present = self.is_present,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            service,
            path,
            is_present,
            is_disabled,
        } = self;

        if *is_disabled {
            execute_command(
                Command::new("launchctl")
                    .process_group(0)
                    .arg("enable")
                    .arg(&format!("{DARWIN_LAUNCHD_DOMAIN}/{service}"))
                    .stdin(std::process::Stdio::null()),
            )
            .await
            .map_err(Self::error)?;
        }

        if !*is_present {
            crate::action::macos::retry_bootstrap(DARWIN_LAUNCHD_DOMAIN, &service, &path)
                .await
                .map_err(Self::error)?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!(
                "Run `launchctl bootout {} {}`",
                DARWIN_LAUNCHD_DOMAIN,
                self.path.display()
            ),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        crate::action::macos::retry_bootout(DARWIN_LAUNCHD_DOMAIN, &self.service, &self.path)
            .await
            .map_err(Self::error)?;

        Ok(())
    }
}
