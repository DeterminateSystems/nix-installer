use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionState, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};

/**
Start a given systemd unit
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
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
        Ok(StatefulAction {
            action: Self {
                unit: unit.as_ref().to_string(),
                enable,
            },
            state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "start_systemd_unit")]
impl Action for StartSystemdUnit {
    fn tracing_synopsis(&self) -> String {
        format!("Enable (and start) the systemd unit {}", self.unit)
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
                        .arg(format!("{unit}"))
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ActionError::Custom(Box::new(StartSystemdUnitError::Command(e))))?;
            },
            false => {
                // TODO(@Hoverbear): Handle proxy vars
                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("start")
                        .arg(format!("{unit}"))
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ActionError::Custom(Box::new(StartSystemdUnitError::Command(e))))?;
            },
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Disable (and stop) the systemd unit {}", self.unit),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self { unit, enable } = self;

        if *enable {
            execute_command(
                Command::new("systemctl")
                    .process_group(0)
                    .arg("disable")
                    .arg(format!("{unit}"))
                    .stdin(std::process::Stdio::null()),
            )
            .await
            .map_err(|e| ActionError::Custom(Box::new(StartSystemdUnitError::Command(e))))?;
        };

        // We do both to avoid an error doing `disable --now` if the user did stop it already somehow.
        execute_command(
            Command::new("systemctl")
                .process_group(0)
                .arg("stop")
                .arg(format!("{unit}"))
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| ActionError::Custom(Box::new(StartSystemdUnitError::Command(e))))?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartSystemdUnitError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
