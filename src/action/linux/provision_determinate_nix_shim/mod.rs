use std::path::PathBuf;

use tokio::fs::{create_dir_all, remove_file};
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag};
use crate::execute_command;

use crate::action::{Action, ActionDescription, StatefulAction};

/**
Provision the determinate-nix-ee binary
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ProvisionDeterminateNixShim {
    binary_location: PathBuf,
    service_location: PathBuf,
}

impl ProvisionDeterminateNixShim {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        crate::settings::DETERMINATE_NIX_BINARY
            .ok_or_else(|| Self::error(ActionErrorKind::DeterminateNixUnavailable))?;

        let this = Self {
            binary_location: "/nix/determinate/determinate-nix-ee".into(),
            service_location: "/nix/determinate/nix-daemon.service".into(),
        };

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "provision_determinate_nix_shim")]
impl Action for ProvisionDeterminateNixShim {
    fn action_tag() -> ActionTag {
        ActionTag("provision_determinate_nix_shim")
    }
    fn tracing_synopsis(&self) -> String {
        "Install Determinate Nix Shim".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "provision_determinate_nix_shim",
            location = ?self.binary_location,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!("Enable Determinate Nix superpowers")],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let bytes = crate::settings::DETERMINATE_NIX_BINARY
            .ok_or_else(|| Self::error(ActionErrorKind::DeterminateNixUnavailable))?;

        if self.binary_location.exists() {
            remove_file(&self.binary_location)
                .await
                .map_err(|e| ActionErrorKind::Remove(self.binary_location.clone(), e))
                .map_err(Self::error)?;
        }

        if let Some(parent) = self.binary_location.parent() {
            create_dir_all(&parent)
                .await
                .map_err(|e| ActionErrorKind::CreateDirectory(parent.into(), e))
                .map_err(Self::error)?;
        }

        tokio::fs::write(&self.binary_location, bytes)
            .await
            .map_err(|e| ActionErrorKind::Write(self.binary_location.clone(), e))
            .map_err(Self::error)?;

        tokio::fs::write(
            &self.service_location,
            include_str!("./nix-daemon.determinate-nix.service"),
        )
        .await
        .map_err(|e| ActionErrorKind::Write(self.service_location.clone(), e))
        .map_err(Self::error)?;

        execute_command(
            Command::new(&self.binary_location)
                .arg("--stop-after")
                .arg("init"),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Remove the Determinate Nix superpowers".into(),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        if self.binary_location.exists() {
            remove_file(&self.binary_location)
                .await
                .map_err(|e| ActionErrorKind::Remove(self.binary_location.clone(), e))
                .map_err(Self::error)?;
        }

        Ok(())
    }
}
