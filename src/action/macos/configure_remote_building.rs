use crate::action::base::{create_or_insert_into_file, CreateOrInsertIntoFile};
use crate::action::{Action, ActionDescription, ActionError, ActionTag, StatefulAction};

use std::path::Path;
use tracing::{span, Instrument, Span};

const PROFILE_NIX_FILE_SHELL: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh";

/**
Configure macOS's zshenv to load the Nix environment when ForceCommand is used.
This enables remote building, which requires `ssh host nix` to work.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureRemoteBuilding {
    create_or_insert_into_file: StatefulAction<CreateOrInsertIntoFile>,
}

impl ConfigureRemoteBuilding {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let shell_buf = format!(
            r#"
# Set up Nix only on SSH connections
# See: https://github.com/DeterminateSystems/nix-installer/pull/714
if [ -e '{PROFILE_NIX_FILE_SHELL}' ] && [ -n "${{SSH_CONNECTION}}" ] && [ "${{SHLVL}}" -eq 1 ]; then
    . '{PROFILE_NIX_FILE_SHELL}'
fi
# End Nix
"#
        );

        let create_or_insert_into_file = CreateOrInsertIntoFile::plan(
            Path::new("/etc/zshenv"),
            None,
            None,
            0o644,
            shell_buf.to_string(),
            create_or_insert_into_file::Position::Beginning,
        )
        .await
        .map_err(Self::error)?;

        Ok(Self {
            create_or_insert_into_file,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "configure_remote_building")]
impl Action for ConfigureRemoteBuilding {
    fn action_tag() -> ActionTag {
        ActionTag("configure_remote_building")
    }
    fn tracing_synopsis(&self) -> String {
        "Configuring zsh to support using Nix in non-interactive shells".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "configure_remote_building",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec!["Update `/etc/zshenv` to import Nix".to_string()],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let span = tracing::Span::current().clone();
        self.create_or_insert_into_file
            .try_execute()
            .instrument(span)
            .await
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Remove the Nix configuration from zsh's non-login shells".to_string(),
            vec!["Update `/etc/zshenv` to no longer import Nix".to_string()],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        self.create_or_insert_into_file.try_revert().await?;

        Ok(())
    }
}
