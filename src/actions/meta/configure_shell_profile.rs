use std::path::Path;

use tokio::task::JoinSet;

use crate::HarmonicError;

use crate::actions::base::{CreateOrAppendFile, CreateOrAppendFileReceipt};
use crate::actions::{ActionDescription, Actionable, Revertable};

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
    pub async fn plan() -> Result<Self, HarmonicError> {
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
impl<'a> Actionable<'a> for ConfigureShellProfile {
    type Receipt = ConfigureShellProfileReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Configure the shell profiles".to_string(),
            vec!["Update shell profiles to import Nix".to_string()],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
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
                Ok(Ok(success)) => successes.push(success),
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

        Ok(Self::Receipt {
            create_or_append_files: successes,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureShellProfileReceipt {
    create_or_append_files: Vec<CreateOrAppendFileReceipt>,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for ConfigureShellProfileReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
