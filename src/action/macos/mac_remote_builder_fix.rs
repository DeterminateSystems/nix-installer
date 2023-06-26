use std::path::{Path, PathBuf};

use tracing::{span, Span};

use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

const NIX_STORE_SYMLINK_LOCATION: &str = "/usr/local/bin/nix-store";
const NIX_STORE_SYMLINK_TARGET: &str = "/nix/var/nix/profiles/default/bin/nix-store";

/**
Perform a remote builder fix for Mac which links `nix-store` into `/usr/local/bin`
*/
// Previously, the installer suggested users use a workaround: https://github.com/DeterminateSystems/nix-installer/blob/4bfd6c2547dab100cde1dbc60e3d623499ead2c4/README.md?plain=1#L377-L418
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct MacRemoteBuilderFix;

impl MacRemoteBuilderFix {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let nix_path = Path::new(NIX_STORE_SYMLINK_LOCATION);
        if nix_path.exists() {
            if !nix_path.is_symlink() {
                return Err(Self::error(ActionErrorKind::PathWasNotSymlink(
                    nix_path.into(),
                )));
            }
            let link_destination = std::fs::read_link(nix_path)
                .map_err(|e| ActionErrorKind::ReadSymlink(nix_path.into(), e))
                .map_err(Self::error)?;
            if link_destination == Path::new("/nix/var/nix/profiles/default/bin/nix-store") {
                return Ok(StatefulAction::completed(Self));
            } else {
                return Err(Self::error(ActionErrorKind::SymlinkExists(nix_path.into())));
            }
        }

        Ok(StatefulAction::uncompleted(Self))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "mac_remote_builder_fix")]
impl Action for MacRemoteBuilderFix {
    fn action_tag() -> ActionTag {
        ActionTag("mac_remote_builder_fix")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create symlink at `{NIX_STORE_SYMLINK_LOCATION}` to `{NIX_STORE_SYMLINK_TARGET}`",
        )
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "mac_remote_builder_fix",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![
            "In order to support acting as a remote builder (Mac populates the `PATH` environment differently than other environments)".to_string()
        ])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        tokio::fs::symlink(NIX_STORE_SYMLINK_TARGET, NIX_STORE_SYMLINK_LOCATION)
            .await
            .map_err(|e| {
                ActionErrorKind::Symlink(
                    PathBuf::from(NIX_STORE_SYMLINK_LOCATION),
                    PathBuf::from(NIX_STORE_SYMLINK_TARGET),
                    e,
                )
            })
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Remove symlink at `{NIX_STORE_SYMLINK_LOCATION}`",),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        tokio::fs::remove_file(NIX_STORE_SYMLINK_LOCATION)
            .await
            .map_err(|e| ActionErrorKind::Remove(PathBuf::from(NIX_STORE_SYMLINK_LOCATION), e))
            .map_err(Self::error)?;

        Ok(())
    }
}
