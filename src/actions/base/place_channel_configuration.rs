use reqwest::Url;
use serde::Serialize;

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

use super::{CreateFile, CreateFileError};

const NIX_CHANNELS_PATH: &str = "/root/.nix-channels";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceChannelConfiguration {
    channels: Vec<(String, Url)>,
    create_file: CreateFile,
    action_state: ActionState,
}

impl PlaceChannelConfiguration {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        channels: Vec<(String, Url)>,
        force: bool,
    ) -> Result<Self, PlaceChannelConfigurationError> {
        let buf = channels
            .iter()
            .map(|(name, url)| format!("{} {}", url, name))
            .collect::<Vec<_>>()
            .join("\n");
        let create_file = CreateFile::plan(
            NIX_CHANNELS_PATH,
            "root".into(),
            "root".into(),
            0o0664,
            buf,
            force,
        )
        .await?;
        Ok(Self {
            create_file,
            channels,
            action_state: ActionState::Planned,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for PlaceChannelConfiguration {
    type Error = PlaceChannelConfigurationError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            channels: _,
            create_file: _,
            action_state: _,
        } = self;
        vec![ActionDescription::new(
            "Place a channel configuration".to_string(),
            vec!["Place a configuration at `{NIX_CHANNELS_PATH}` setting the channels".to_string()],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            create_file,
            channels: _,
            action_state,
        } = self;

        create_file.execute().await?;

        *action_state = ActionState::Completed;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            create_file,
            channels: _,
            action_state,
        } = self;

        create_file.revert().await?;

        *action_state = ActionState::Reverted;
        Ok(())
    }
}

impl From<PlaceChannelConfiguration> for Action {
    fn from(v: PlaceChannelConfiguration) -> Self {
        Action::PlaceChannelConfiguration(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum PlaceChannelConfigurationError {
    #[error(transparent)]
    CreateFile(#[from] CreateFileError),
}
