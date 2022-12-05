use crate::action::base::CreateFile;
use crate::action::ActionError;
use crate::action::{Action, ActionDescription, StatefulAction};
use reqwest::Url;

/**
Place a channel configuration containing `channels` to the `$ROOT_HOME/.nix-channels` file
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceChannelConfiguration {
    channels: Vec<(String, Url)>,
    create_file: StatefulAction<CreateFile>,
}

impl PlaceChannelConfiguration {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        channels: Vec<(String, Url)>,
        force: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let buf = channels
            .iter()
            .map(|(name, url)| format!("{} {}", url, name))
            .collect::<Vec<_>>()
            .join("\n");
        let create_file = CreateFile::plan(
            dirs::home_dir()
                .ok_or_else(|| {
                    ActionError::Custom(Box::new(PlaceChannelConfigurationError::NoRootHome))
                })?
                .join(".nix-channels"),
            None,
            None,
            0o0664,
            buf,
            force,
        )
        .await?;
        Ok(Self {
            create_file,
            channels,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "place_channel_configuration")]
impl Action for PlaceChannelConfiguration {
    fn tracing_synopsis(&self) -> String {
        format!(
            "Place channel configuration at `{}`",
            self.create_file.inner().path.display()
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        channels = self.channels.iter().map(|(c, u)| format!("{c}={u}")).collect::<Vec<_>>().join(", "),
    ))]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            create_file,
            channels: _,
        } = self;

        create_file.try_execute().await?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!(
                "Remove channel configuration at `{}`",
                self.create_file.inner().path.display()
            ),
            vec![],
        )]
    }

    #[tracing::instrument(skip_all, fields(
        channels = self.channels.iter().map(|(c, u)| format!("{c}={u}")).collect::<Vec<_>>().join(", "),
    ))]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            create_file,
            channels: _,
        } = self;

        create_file.try_revert().await?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlaceChannelConfigurationError {
    #[error("No root home found to place channel configuration in")]
    NoRootHome,
}
