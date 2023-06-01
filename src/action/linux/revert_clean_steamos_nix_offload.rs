use std::path::Path;

use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag};

use crate::action::{Action, ActionDescription, StatefulAction};

const OFFLOAD_PATH: &'static str = "/home/.steamos/offload/nix";

/**
Clean out the `/home/.steamos/offload/nix`

In SteamOS build ID 20230522.1000 (and, presumably, later) a `/home/.steamos/offload/nix` directory
exists by default and needs to be cleaned out on uninstall, otherwise uninstall won't work.
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct RevertCleanSteamosNixOffload;

impl RevertCleanSteamosNixOffload {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        if Path::new(OFFLOAD_PATH).exists() {
            Ok(StatefulAction::uncompleted(RevertCleanSteamosNixOffload))
        } else {
            Ok(StatefulAction::completed(RevertCleanSteamosNixOffload))
        }
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "revert_clean_steamos_nix_offload")]
impl Action for RevertCleanSteamosNixOffload {
    fn action_tag() -> ActionTag {
        ActionTag("revert_clean_steamos_nix_offload")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Clean the `{OFFLOAD_PATH}` directory")
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "revert_clean_steamos_nix_offload",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // noop

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![
                format!("On more recent versions of SteamOS, the `{OFFLOAD_PATH}` folder contains the Nix store, and needs to be cleaned on uninstall."),
            ],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let paths = glob::glob(OFFLOAD_PATH).map_err(Self::error)?;

        for path in paths {
            let path = path.map_err(Self::error)?;
            tracing::trace!(path = %path.display(), "Removing");
            tokio::fs::remove_dir_all(&path)
                .await
                .map_err(|e| Self::error(ActionErrorKind::Remove(path.into(), e)))?;
        }

        Ok(())
    }
}
