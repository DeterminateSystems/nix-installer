use std::path::Path;

use serde::Serialize;
use tokio::task::{JoinError, JoinSet};

use crate::actions::base::{CreateOrAppendFile, CreateOrAppendFileError};
use crate::actions::{Action, ActionDescription, ActionState, Actionable};

const PROFILE_TARGETS: &[&str] = &[
    "/etc/bashrc",
    "/etc/profile.d/nix.sh",
    "/etc/zshrc",
    "/etc/bash.bashrc",
    "/etc/zsh/zshrc",
    // TODO(@hoverbear): FIsh
];
const PROFILE_NIX_FILE: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureShellProfile {
    create_or_append_files: Vec<CreateOrAppendFile>,
    action_state: ActionState,
}

impl ConfigureShellProfile {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<Self, ConfigureShellProfileError> {
        let mut create_or_append_files = Vec::default();
        for profile_target in PROFILE_TARGETS {
            let path = Path::new(profile_target);
            if !path.exists() {
                tracing::trace!("Did not plan to edit `{profile_target}` as it does not exist.");
                continue;
            }
            let buf = format!(
                "\n\
                # Nix\n\
                if [ -e '{PROFILE_NIX_FILE}' ]; then\n\
                . '{PROFILE_NIX_FILE}'\n\
                fi\n\
                # End Nix\n
            \n",
            );
            create_or_append_files.push(
                CreateOrAppendFile::plan(path, "root".to_string(), "root".to_string(), 0o0644, buf)
                    .await?,
            );
        }

        Ok(Self {
            create_or_append_files,
            action_state: ActionState::Planned,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ConfigureShellProfile {
    type Error = ConfigureShellProfileError;
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Configure the shell profiles".to_string(),
            vec!["Update shell profiles to import Nix".to_string()],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            create_or_append_files,
            action_state,
        } = self;
        tracing::info!("Configuring shell profile");

        let mut set = JoinSet::new();
        let mut errors = Vec::default();

        for (idx, create_or_append_file) in create_or_append_files.iter().enumerate() {
            let mut create_or_append_file_clone = create_or_append_file.clone();
            let _abort_handle = set.spawn(async move {
                create_or_append_file_clone.execute().await?;
                Result::<_, CreateOrAppendFileError>::Ok((idx, create_or_append_file_clone))
            });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((idx, create_or_append_file))) => {
                    create_or_append_files[idx] = create_or_append_file
                },
                Ok(Err(e)) => errors.push(e),
                Err(e) => return Err(e.into()),
            };
        }

        if !errors.is_empty() {
            if errors.len() == 1 {
                return Err(errors.into_iter().next().unwrap().into());
            } else {
                return Err(ConfigureShellProfileError::MultipleCreateOrAppendFile(
                    errors,
                ));
            }
        }

        *action_state = ActionState::Completed;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            create_or_append_files,
            action_state,
        } = self;
        tracing::info!("Configuring shell profile");

        let mut set = JoinSet::new();
        let mut errors = Vec::default();

        for (idx, create_or_append_file) in create_or_append_files.iter().enumerate() {
            let mut create_or_append_file_clone = create_or_append_file.clone();
            let _abort_handle = set.spawn(async move {
                create_or_append_file_clone.revert().await?;
                Result::<_, CreateOrAppendFileError>::Ok((idx, create_or_append_file_clone))
            });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok((idx, create_or_append_file))) => {
                    create_or_append_files[idx] = create_or_append_file
                },
                Ok(Err(e)) => errors.push(e),
                Err(e) => return Err(e.into()),
            };
        }

        if !errors.is_empty() {
            if errors.len() == 1 {
                return Err(errors.into_iter().next().unwrap().into());
            } else {
                return Err(ConfigureShellProfileError::MultipleCreateOrAppendFile(
                    errors,
                ));
            }
        }

        *action_state = ActionState::Reverted;
        Ok(())
    }
}

impl From<ConfigureShellProfile> for Action {
    fn from(v: ConfigureShellProfile) -> Self {
        Action::ConfigureShellProfile(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum ConfigureShellProfileError {
    #[error(transparent)]
    CreateOrAppendFile(#[from] CreateOrAppendFileError),
    #[error("Multiple errors: {}", .0.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" & "))]
    MultipleCreateOrAppendFile(Vec<CreateOrAppendFileError>),
    #[error(transparent)]
    Join(
        #[from]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        JoinError,
    ),
}
