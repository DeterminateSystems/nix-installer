use std::path::PathBuf;

use crate::{
    action::{ActionError, ActionErrorKind, ActionTag, StatefulAction},
    execute_command,
};

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{Action, ActionDescription};

use crate::action::base::CreateFile;

use super::ConfigureNix;

/**
Setup the default system channel with nixpkgs-unstable.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SetupChannels {
    create_file: StatefulAction<CreateFile>,
    unpacked_path: PathBuf,
}

impl SetupChannels {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(unpacked_path: PathBuf) -> Result<StatefulAction<Self>, ActionError> {
        let create_file = CreateFile::plan(
            dirs::home_dir()
                .ok_or_else(|| Self::error(SetupChannelsError::NoRootHome))?
                .join(".nix-channels"),
            None,
            None,
            0o664,
            "https://nixos.org/channels/nixpkgs-unstable nixpkgs\n".to_string(),
            false,
        )
        .await?;
        Ok(Self {
            create_file,
            unpacked_path,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "setup_channels")]
impl Action for SetupChannels {
    fn action_tag() -> ActionTag {
        ActionTag("setup_channels")
    }
    fn tracing_synopsis(&self) -> String {
        "Setup the default system channel".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "setup_channels",
            unpacked_path = %self.unpacked_path.display(),
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let mut explanation = vec![];

        if let Some(val) = self.create_file.describe_execute().first() {
            explanation.push(val.description.clone())
        }

        explanation.push("Run `nix-channel --update nixpkgs`".to_string());

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // Place channel configuration
        self.create_file.try_execute().await?;

        let (nix_pkg, nss_ca_cert_pkg) =
            ConfigureNix::find_nix_and_ca_cert(&self.unpacked_path).await?;
        // Update nixpkgs channel
        execute_command(
            Command::new(nix_pkg.join("bin/nix-channel"))
                .process_group(0)
                .arg("--update")
                .arg("nixpkgs")
                .stdin(std::process::Stdio::null())
                .env(
                    "HOME",
                    dirs::home_dir().ok_or_else(|| Self::error(SetupChannelsError::NoRootHome))?,
                )
                .env(
                    "NIX_SSL_CERT_FILE",
                    nss_ca_cert_pkg.join("etc/ssl/certs/ca-bundle.crt"),
                ), /* We could rely on setup_default_profile setting this
                   environment variable, but add this just to be explicit. */
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Remove system channel configuration".to_string(),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        self.create_file.try_revert().await?;

        // We could try to rollback
        // /nix/var/nix/profiles/per-user/root/channels, but that will happen
        // anyways when /nix gets cleaned up.

        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum SetupChannelsError {
    #[error("No root home found to place channel configuration in")]
    NoRootHome,
}

impl From<SetupChannelsError> for ActionErrorKind {
    fn from(val: SetupChannelsError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}
