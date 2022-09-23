use reqwest::Url;
use serde::Serialize;

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, ActionState, Action};

use super::{CreateFile, CreateFileError};

const NIX_CHANNELS_PATH: &str = "/root/.nix-channels";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceChannelConfiguration {
    channels: Vec<(String, Url)>,
    create_file: CreateFile,
}

impl PlaceChannelConfiguration {
    #[tracing::instrument(skip_all)]
    pub async fn plan(channels: Vec<(String, Url)>, force: bool) -> Result<Self, HarmonicError> {
        let buf = channels
            .iter()
            .map(|(name, url)| format!("{} {}", url, name))
            .collect::<Vec<_>>()
            .join("\n");
        let create_file =
            CreateFile::plan(NIX_CHANNELS_PATH, "root".into(), "root".into(), 0o0664, buf, force).await?;
        Ok(Self {
            create_file,
            channels,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<PlaceChannelConfiguration> {
    type Error = PlaceChannelConfigurationError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            channels,
             create_file,
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
            channels,
        } = self;
        
        create_file.execute().await?;
        
        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<PlaceChannelConfiguration>> for ActionState<Action> {
    fn from(v: ActionState<PlaceChannelConfiguration>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::PlaceChannelConfiguration(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::PlaceChannelConfiguration(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::PlaceChannelConfiguration(v)),
        }
    }
}


#[derive(Debug, thiserror::Error, Serialize)]
pub enum PlaceChannelConfigurationError {

}
