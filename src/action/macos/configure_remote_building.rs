use crate::action::base::{create_or_insert_into_file, CreateOrInsertIntoFile};
use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

use std::path::Path;
use tokio::task::JoinSet;
use tracing::{span, Instrument, Span};

const PROFILE_NIX_FILE_SHELL: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh";

/**
Configure macOS's zshenv to load the Nix environment when ForceCommand is used.
This enables remote building, which requires `ssh host nix` to work.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureRemoteBuilding {
    create_or_insert_into_files: Vec<StatefulAction<CreateOrInsertIntoFile>>,
}

impl ConfigureRemoteBuilding {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let mut create_or_insert_into_files = Vec::default();

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

        let profile_target_path = Path::new("/etc/zshenv");
        create_or_insert_into_files.push(
            CreateOrInsertIntoFile::plan(
                profile_target_path,
                None,
                None,
                0o644,
                shell_buf.to_string(),
                create_or_insert_into_file::Position::Beginning,
            )
            .await
            .map_err(Self::error)?,
        );

        Ok(Self {
            create_or_insert_into_files,
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
        let mut set = JoinSet::new();
        let mut errors = vec![];

        for (idx, create_or_insert_into_file) in
            self.create_or_insert_into_files.iter_mut().enumerate()
        {
            let span = tracing::Span::current().clone();
            let mut create_or_insert_into_file_clone = create_or_insert_into_file.clone();
            let _abort_handle = set.spawn(async move {
                create_or_insert_into_file_clone
                    .try_execute()
                    .instrument(span)
                    .await
                    .map_err(Self::error)?;
                Result::<_, ActionError>::Ok((idx, create_or_insert_into_file_clone))
            });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((idx, create_or_insert_into_file))) => {
                    self.create_or_insert_into_files[idx] = create_or_insert_into_file
                },
                Ok(Err(e)) => errors.push(e),
                Err(e) => return Err(Self::error(e))?,
            };
        }

        if !errors.is_empty() {
            if errors.len() == 1 {
                return Err(Self::error(errors.into_iter().next().unwrap()))?;
            } else {
                return Err(Self::error(ActionErrorKind::MultipleChildren(
                    errors.into_iter().collect(),
                )));
            }
        }

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
        let mut set = JoinSet::new();
        let mut errors = vec![];

        for (idx, create_or_insert_into_file) in
            self.create_or_insert_into_files.iter_mut().enumerate()
        {
            let mut create_or_insert_file_clone = create_or_insert_into_file.clone();
            let _abort_handle = set.spawn(async move {
                create_or_insert_file_clone.try_revert().await?;
                Result::<_, _>::Ok((idx, create_or_insert_file_clone))
            });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((idx, create_or_insert_into_file))) => {
                    self.create_or_insert_into_files[idx] = create_or_insert_into_file
                },
                Ok(Err(e)) => errors.push(e),
                // This is quite rare and generally a very bad sign.
                Err(e) => return Err(e).map_err(|e| Self::error(ActionErrorKind::from(e)))?,
            };
        }

        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(errors
                .into_iter()
                .next()
                .expect("Expected 1 len Vec to have at least 1 item"))
        } else {
            Err(Self::error(ActionErrorKind::MultipleChildren(errors)))
        }
    }
}
