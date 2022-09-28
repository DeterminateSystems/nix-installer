use serde::Serialize;

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

use crate::actions::base::{CreateDirectory, CreateDirectoryError, CreateFile, CreateFileError};

const NIX_CONF_FOLDER: &str = "/etc/nix";
const NIX_CONF: &str = "/etc/nix/nix.conf";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceNixConfiguration {
    create_directory: CreateDirectory,
    create_file: CreateFile,
    action_state: ActionState,
}

impl PlaceNixConfiguration {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        nix_build_group_name: String,
        extra_conf: Option<String>,
        force: bool,
    ) -> Result<Self, PlaceNixConfigurationError> {
        let buf = format!(
            "\
            {extra_conf}\n\
            build-users-group = {nix_build_group_name}\n\
            \n\
            experimental-features = nix-command flakes\n\
            \n\
            auto-optimise-store = true\n\
        ",
            extra_conf = extra_conf.unwrap_or_else(|| "".into()),
        );
        let create_directory =
            CreateDirectory::plan(NIX_CONF_FOLDER, "root".into(), "root".into(), 0o0755, force)
                .await?;
        let create_file =
            CreateFile::plan(NIX_CONF, "root".into(), "root".into(), 0o0664, buf, force).await?;
        Ok(Self {
            create_directory,
            create_file,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for PlaceNixConfiguration {
    type Error = PlaceNixConfigurationError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Place the nix configuration in `{NIX_CONF}`"),
                vec![
                    "This file is read by the Nix daemon to set its configuration options at runtime."
                        .to_string(),
                ],
            )]
        }
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            create_file,
            create_directory,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Placing Nix configuration");
            return Ok(());
        }
        *action_state = ActionState::Progress;
        tracing::debug!("Placing Nix configuration");

        create_directory.execute().await?;
        create_file.execute().await?;

        tracing::trace!("Placed Nix configuration");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Remove the nix configuration in `{NIX_CONF}`"),
                vec![
                    "This file is read by the Nix daemon to set its configuration options at runtime."
                        .to_string(),
                ],
            )]
        }
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            create_file,
            create_directory,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Remove nix configuration");
            return Ok(());
        }
        *action_state = ActionState::Progress;
        tracing::debug!("Remove nix configuration");

        create_file.revert().await?;
        create_directory.revert().await?;

        tracing::trace!("Removed nix configuration");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }
}

impl From<PlaceNixConfiguration> for Action {
    fn from(v: PlaceNixConfiguration) -> Self {
        Action::PlaceNixConfiguration(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum PlaceNixConfigurationError {
    #[error("Creating file")]
    CreateFile(#[source] #[from] CreateFileError),
    #[error("Creating directory")]
    CreateDirectory(#[source] #[from] CreateDirectoryError),
}
