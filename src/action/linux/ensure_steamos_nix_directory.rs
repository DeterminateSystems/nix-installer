use std::path::{Path, PathBuf};

use tokio::fs::create_dir;
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag};
use crate::execute_command;

use crate::action::{Action, ActionDescription, StatefulAction};

/**
Ensure SeamOS's `/nix` folder exists.

In SteamOS build ID 20230522.1000 (and, presumably, later) a `/nix` directory and related units
exist. In previous versions of `nix-installer` the uninstall process would remove that directory.
This action ensures that the folder does indeed exist.
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct EnsureSteamosNixDirectory;

impl EnsureSteamosNixDirectory {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        if which::which("steamos-readonly").is_err() {
            return Err(Self::error(ActionErrorKind::MissingSteamosBinary(
                "steamos-readonly".into(),
            )));
        }
        if Path::new("/nix").exists() {
            Ok(StatefulAction::completed(EnsureSteamosNixDirectory))
        } else {
            Ok(StatefulAction::uncompleted(EnsureSteamosNixDirectory))
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "ensure_steamos_nix_directory")]
impl Action for EnsureSteamosNixDirectory {
    fn action_tag() -> ActionTag {
        ActionTag("ensure_steamos_nix_directory")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Ensure SteamOS's `/nix` directory exists")
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "ensure_steamos_nix_directory",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![
                "On more recent versions of SteamOS, a `/nix` folder now exists on the base image.".to_string(),
                "Previously, `nix-installer` created this directory through systemd units.".to_string(),
                "It's likely you updated SteamOS, then ran `/nix/nix-installer uninstall`, which deleted the `/nix` directory.".to_string(),
            ],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        execute_command(
            Command::new("steamos-readonly")
                .process_group(0)
                .arg("disable")
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        let path = PathBuf::from("/nix");
        create_dir(&path)
            .await
            .map_err(|e| ActionErrorKind::CreateDirectory(path.clone(), e))
            .map_err(Self::error)?;

        execute_command(
            Command::new("steamos-readonly")
                .process_group(0)
                .arg("enable")
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        // noop

        Ok(())
    }
}
