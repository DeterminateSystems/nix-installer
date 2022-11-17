use tokio::process::Command;

use crate::execute_command;

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct StartSystemdUnit {
    unit: String,
    action_state: ActionState,
}

impl StartSystemdUnit {
    #[tracing::instrument(skip_all)]
    pub async fn plan(unit: String) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            unit,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "start_systemd_unit")]
impl Action for StartSystemdUnit {
    fn tracing_synopsis(&self) -> String {
        "Start the systemd Nix service and socket".to_string()
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![
                "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
            ]
        )]
    }

    #[tracing::instrument(skip_all, fields(
        unit = %self.unit,
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { unit, .. } = self;

        // TODO(@Hoverbear): Handle proxy vars
        execute_command(
            Command::new("systemctl")
                .arg("enable")
                .arg("--now")
                .arg(format!("{unit}"))
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| StartSystemdUnitError::Command(e).boxed())?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Stop the systemd Nix service and socket".to_string(),
            vec![
                "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
            ]
        )]
    }

    #[tracing::instrument(skip_all, fields(
        unit = %self.unit,
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { unit, .. } = self;

        // TODO(@Hoverbear): Handle proxy vars
        execute_command(
            Command::new("systemctl")
                .arg("disable")
                .arg(format!("{unit}"))
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| StartSystemdUnitError::Command(e).boxed())?;

        // We do both to avoid an error doing `disable --now` if the user did stop it already somehow.
        execute_command(
            Command::new("systemctl")
                .arg("stop")
                .arg(format!("{unit}"))
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(|e| StartSystemdUnitError::Command(e).boxed())?;

        Ok(())
    }

    fn action_state(&self) -> ActionState {
        self.action_state
    }

    fn set_action_state(&mut self, action_state: ActionState) {
        self.action_state = action_state;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartSystemdUnitError {
    #[error("Failed to execute command")]
    Command(#[source] std::io::Error),
}
