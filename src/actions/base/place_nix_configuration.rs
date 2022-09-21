use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

use super::{CreateFile, CreateFileReceipt};

const NIX_CONF: &str = "/etc/nix/nix.conf";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceNixConfiguration {
    create_file: CreateFile,
}

impl PlaceNixConfiguration {
    pub async fn plan(nix_build_group_name: String, extra_conf: Option<String>) -> Result<Self, HarmonicError> {
        let buf = format!(
            "\
            {extra_conf}\n\
            build-users-group = {nix_build_group_name}\n\
        ",
            extra_conf = extra_conf.unwrap_or_else(|| "".into()),
        );
        let create_file = CreateFile::plan(NIX_CONF, "root".into(), "root".into(), 0o0664, buf).await?;
        Ok(Self { create_file })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for PlaceNixConfiguration {
    type Receipt = PlaceNixConfigurationReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Place the nix configuration".to_string(),
                vec![
                    "Boop".to_string()
                ]
            ),
        ]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { create_file } = self;
        let create_file = create_file.execute().await?;
        Ok(Self::Receipt { create_file })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceNixConfigurationReceipt {
    create_file: CreateFileReceipt,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for PlaceNixConfigurationReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
