use std::path::Path;

use reqwest::Url;

use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

use super::{CreateOrAppendFile, CreateFile, CreateFileReceipt};

const NIX_CHANNELS_PATH: &str = "/root/.nix-channels";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceChannelConfiguration {
    channels: Vec<(String, Url)>,
    create_file: CreateFile,
}

impl PlaceChannelConfiguration {
    pub async fn plan(channels: Vec<(String, Url)>) -> Result<Self, HarmonicError> {
        let buf = channels
            .iter()
            .map(|(name, url)| format!("{} {}", url, name))
            .collect::<Vec<_>>()
            .join("\n");
        let create_file = CreateFile::plan(NIX_CHANNELS_PATH, "root".into(), "root".into(), 0o0664, buf).await?;
        Ok(Self { create_file, channels })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for PlaceChannelConfiguration {
    type Receipt = PlaceChannelConfigurationReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { channels, create_file } = self;
        vec![
            ActionDescription::new(
                "Place a channel configuration".to_string(),
                vec![
                    "Place a configuration at `{NIX_CHANNELS_PATH}` setting the channels".to_string()
                ]
            ),
        ]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { create_file, channels } = self;
        let create_file = create_file.execute().await?;
        Ok(Self::Receipt { create_file, channels })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceChannelConfigurationReceipt {
    channels: Vec<(String, Url)>,
    create_file: CreateFileReceipt,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for PlaceChannelConfigurationReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
