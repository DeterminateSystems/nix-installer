use std::path::Path;

use serde::Serialize;
use tokio::task::JoinSet;

use crate::HarmonicError;

use crate::actions::base::{CreateOrAppendFile, CreateOrAppendFileError};
use crate::actions::{ActionDescription, Actionable, ActionState, Action};

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
}

impl ConfigureShellProfile {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<ActionState<Self>, ConfigureShellProfileError> {
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
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<ConfigureShellProfile> {
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
        } = self;
        tracing::info!("Configuring shell profile");

        let mut set = JoinSet::new();

        let mut successes = Vec::with_capacity(create_or_append_files.len());
        let mut errors = Vec::default();

        for create_or_append_file in create_or_append_files {
            let _abort_handle = set.spawn(async move { create_or_append_file.execute().await });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok(())) => (),
                Ok(Err(e)) => errors.push(e),
                Err(e) => errors.push(e.into()),
            };
        }

        if !errors.is_empty() {
            if errors.len() == 1 {
                return Err(errors.into_iter().next().unwrap());
            } else {
                return Err(HarmonicError::Multiple(errors));
            }
        }

        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<ConfigureShellProfile>> for ActionState<Action> {
    fn from(v: ActionState<ConfigureShellProfile>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::ConfigureShellProfile(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::ConfigureShellProfile(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::ConfigureShellProfile(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum ConfigureShellProfileError {

}
