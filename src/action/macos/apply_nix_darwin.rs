use tokio::process::Command;
use tracing::{span, Span};

use crate::{
    action::{Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction},
    execute_command,
};

/// TODO
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ApplyNixDarwin {
    nix_darwin_flake_ref: String,
}

impl ApplyNixDarwin {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(nix_darwin_flake_ref: String) -> Result<StatefulAction<Self>, ActionError> {
        Ok(StatefulAction::uncompleted(Self {
            nix_darwin_flake_ref,
        }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "apply_nix_darwin")]
impl Action for ApplyNixDarwin {
    fn action_tag() -> ActionTag {
        ActionTag("apply_nix_darwin")
    }

    fn tracing_synopsis(&self) -> String {
        format!(
            "Apply the nix-darwin configuration from the flake ref {}`",
            self.nix_darwin_flake_ref,
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "apply_nix_darwin",
            nix_darwin_flake_ref = self.nix_darwin_flake_ref,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // Create and cd into a temporary directory
        let temp_dir = std::env::temp_dir().to_string_lossy().to_string();
        execute_command(
            Command::new("cd")
                .process_group(0)
                .arg(&temp_dir)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        // Build the flake ref into a local result symlink
        execute_command(
            Command::new("nix")
                .process_group(0)
                .args(["build", &self.nix_darwin_flake_ref])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        // Run the built activation script
        execute_command(
            Command::new("./result/sw/bin/darwin-rebuild")
                .process_group(0)
                .arg("activate")
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        // cd into $HOME
        let home = dirs::home_dir().ok_or_else(|| Self::error(ActionErrorKind::NoHomeDirectory))?;

        execute_command(
            Command::new("cd")
                .process_group(0)
                .arg(home)
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        // delete previously created temporary directory
        execute_command(
            Command::new("rm")
                .process_group(0)
                .args(["-rf", &temp_dir])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        // TODO: source shell rc file

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!(
                "Revert nix-darwin apply for the flake ref {}",
                self.nix_darwin_flake_ref,
            ),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        // TODO: deal with shell rc files

        execute_command(
            Command::new("nix")
                .process_group(0)
                // TODO: make the Git ref configurable
                .args(["run", "github:lnL7/nix-darwin/bcc8afd06e237df060c85bad6af7128e05fd61a3#darwin-uninstaller"])
                .stdin(std::process::Stdio::null()),
        )
        .await
        .map_err(Self::error)?;

        Ok(())
    }
}
