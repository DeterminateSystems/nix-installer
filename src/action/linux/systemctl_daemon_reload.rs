use std::path::Path;

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag};
use crate::execute_command;

use crate::action::{Action, ActionDescription, StatefulAction};

/**
Run `systemctl daemon-reload` (on both execute and revert)
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SystemctlDaemonReload;

impl SystemctlDaemonReload {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        if !Path::new("/run/systemd/system").exists() {
            return Err(Self::error(ActionErrorKind::SystemdMissing));
        }

        if which::which("systemctl").is_err() {
            return Err(Self::error(ActionErrorKind::SystemdMissing));
        }

        Ok(StatefulAction::uncompleted(SystemctlDaemonReload))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "systemctl_daemon_reload")]
impl Action for SystemctlDaemonReload {
    fn action_tag() -> ActionTag {
        ActionTag("systemctl_daemon_reload")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Run `systemctl daemon-reload`")
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "systemctl_daemon_reload",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        execute_command(
            Command::new("systemctl")
                .process_group(0)
                .arg("daemon-reload")
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        execute_command(
            Command::new("systemctl")
                .process_group(0)
                .arg("daemon-reload")
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }
}
