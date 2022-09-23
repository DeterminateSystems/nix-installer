use reqwest::Url;

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, Revertable};

use super::{CreateFile, CreateFileReceipt};

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
impl Actionable for PlaceChannelConfiguration {
    type Receipt = PlaceChannelConfigurationReceipt;
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
    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self {
            create_file,
            channels,
        } = self;
        let create_file = create_file.execute().await?;
        Ok(Self::Receipt {
            create_file,
            channels,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceChannelConfigurationReceipt {
    channels: Vec<(String, Url)>,
    create_file: CreateFileReceipt,
}

#[async_trait::async_trait]
impl Revertable for PlaceChannelConfigurationReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
