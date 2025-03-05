use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tracing::{span, Span};

use crate::{
    action::{Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction},
    util::OnMissing,
};

use super::place_nix_configuration::{NIX_CONF, NIX_CONF_FOLDER};

const DETERMINATE_NIXD_BINARY_PATH: &str = "/usr/local/bin/determinate-nixd";
/**
Provision the determinate-nixd binary
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "provision_determinate_nixd")]
pub struct ProvisionDeterminateNixd {
    binary_location: PathBuf,
}

impl ProvisionDeterminateNixd {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        crate::distribution::DETERMINATE_NIXD_BINARY
            .ok_or_else(|| Self::error(ActionErrorKind::DeterminateNixUnavailable))?;

        let this = Self {
            binary_location: DETERMINATE_NIXD_BINARY_PATH.into(),
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
        let bytes = crate::distribution::DETERMINATE_NIXD_BINARY
            .ok_or_else(|| Self::error(ActionErrorKind::DeterminateNixUnavailable))?;

        crate::util::remove_file(&self.binary_location, OnMissing::Ignore)
            .await
            .map_err(|e| ActionErrorKind::Remove(self.binary_location.clone(), e))
            .map_err(Self::error)?;

        if let Some(parent) = self.binary_location.parent() {
            tokio::fs::create_dir_all(&parent)
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
        crate::util::remove_file(&self.binary_location, OnMissing::Ignore)
            .await
            .map_err(|e| ActionErrorKind::Remove(self.binary_location.clone(), e))
            .map_err(Self::error)?;

        // NOTE(cole-h): If /etc/nix/nix.conf exists and we're reverting Determinate, we can safely
        // remove it, since determinate-nixd manages it.
        let nix_conf_path = PathBuf::from(NIX_CONF);
        crate::util::remove_file(&nix_conf_path, OnMissing::Ignore)
            .await
            .map_err(|e| ActionErrorKind::Remove(nix_conf_path, e))
            .map_err(Self::error)?;

        // NOTE(cole-h): If /etc/nix/nix.conf was the last file in /etc/nix, then let's clean up the
        // entire directory too.
        let nix_conf_dir = PathBuf::from(NIX_CONF_FOLDER);
        if let Ok(mut entries) = tokio::fs::read_dir(&nix_conf_dir).await {
            if entries.next_entry().await.ok().flatten().is_none() {
                crate::util::remove_dir_all(&nix_conf_dir, OnMissing::Ignore)
                    .await
                    .map_err(|e| ActionErrorKind::Remove(nix_conf_dir, e))
                    .map_err(Self::error)?;
            }
        }

        Ok(())
    }
}
