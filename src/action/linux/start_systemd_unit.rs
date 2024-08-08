use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionState, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};

/**
Start a given systemd unit
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "start_systemd_unit")]
pub struct StartSystemdUnit {
    unit: String,
    enable: bool,
}

impl StartSystemdUnit {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        unit: impl AsRef<str>,
        enable: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let unit = unit.as_ref();
        let mut command = Command::new("systemctl");
        command.arg("is-active");
        command.arg(unit);
        let output = command
            .output()
            .await
            .map_err(|e| Self::error(ActionErrorKind::command(&command, e)))?;

        let state = if output.status.success() {
            tracing::debug!("Starting systemd unit `{}` already complete", unit);
            ActionState::Skipped
        } else {
            ActionState::Uncompleted
        };

        Ok(StatefulAction {
            action: Self {
                unit: unit.to_string(),
                enable,
            },
            state,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "start_systemd_unit")]
impl Action for StartSystemdUnit {
    fn action_tag() -> ActionTag {
        ActionTag("start_systemd_unit")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Enable (and start) the systemd unit `{}`", self.unit)
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "start_systemd_unit",
            unit = %self.unit,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { unit, enable } = self;

        match enable {
            true => {
                // TODO(@Hoverbear): Handle proxy vars
                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("enable")
                        .arg("--now")
                        .arg(&unit)
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(Self::error)?;
            },
            false => {
                // TODO(@Hoverbear): Handle proxy vars
                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("start")
                        .arg(&unit)
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(Self::error)?;
            },
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Disable (and stop) the systemd unit `{}`", self.unit),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];

        if self.enable {
            if let Err(e) = execute_command(
                Command::new("systemctl")
                    .process_group(0)
                    .arg("disable")
                    .arg(&self.unit)
                    .stdin(std::process::Stdio::null()),
            )
            .await
            .map_err(Self::error)
            {
                errors.push(e);
            }
        };

        // We do both to avoid an error doing `disable --now` if the user did stop it already somehow.
        if let Err(e) = execute_command(
            Command::new("systemctl")
                .process_group(0)
                .arg("stop")
                .arg(&self.unit)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)
        {
            errors.push(e);
        }

        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(errors
                .into_iter()
                .next()
                .expect("Expected 1 len Vec to have at least 1 item"))
        } else {
            Err(Self::error(ActionErrorKind::MultipleChildren(errors)))
        }
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum StartSystemdUnitError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
