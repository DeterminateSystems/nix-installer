use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tokio::fs::{create_dir_all, remove_file};
use tracing::{span, Span};

use crate::action::common::configure_determinate_nixd_init_service::DETERMINATE_NIXD_SERVICE_SRC;
use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

const DETERMINATE_NIXD_BINARY_PATH: &str = "/nix/determinate/determinate-nixd";
/**
Provision the determinate-nixd binary
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "provision_determinate_nixd")]
pub struct ProvisionDeterminateNixd {
    binary_location: PathBuf,
    service_location: PathBuf,
}

impl ProvisionDeterminateNixd {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        crate::settings::DETERMINATE_NIXD_BINARY
            .ok_or_else(|| Self::error(ActionErrorKind::DeterminateNixUnavailable))?;

        let this = Self {
            binary_location: DETERMINATE_NIXD_BINARY_PATH.into(),
            service_location: DETERMINATE_NIXD_SERVICE_SRC.into(),
        };

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "provision_determinate_nixd")]
impl Action for ProvisionDeterminateNixd {
    fn action_tag() -> ActionTag {
        ActionTag("provision_determinate_nixd")
    }
    fn tracing_synopsis(&self) -> String {
        "Install Determinate Nixd".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "provision_determinate_nixd",
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
        let bytes = crate::settings::DETERMINATE_NIXD_BINARY
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

        tokio::fs::set_permissions(&self.binary_location, PermissionsExt::from_mode(0o555))
            .await
            .map_err(|e| ActionErrorKind::Write(self.binary_location.clone(), e))
            .map_err(Self::error)?;

        tokio::fs::write(
            &self.service_location,
            include_str!("./nix-daemon.determinate-nixd.service"),
        )
        .await
        .map_err(|e| ActionErrorKind::Write(self.service_location.clone(), e))
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
