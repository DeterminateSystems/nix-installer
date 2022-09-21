use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, Revertable};

use super::{CreateFile, CreateFileReceipt, CreateDirectory, CreateDirectoryReceipt};

const NIX_CONF_FOLDER: &str = "/etc/nix";
const NIX_CONF: &str = "/etc/nix/nix.conf";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceNixConfiguration {
    create_directory: CreateDirectory,
    create_file: CreateFile,
}

impl PlaceNixConfiguration {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        nix_build_group_name: String,
        extra_conf: Option<String>,
        force: bool,
    ) -> Result<Self, HarmonicError> {
        let buf = format!(
            "\
            {extra_conf}\n\
            build-users-group = {nix_build_group_name}\n\
        ",
            extra_conf = extra_conf.unwrap_or_else(|| "".into()),
        );
        let create_directory = CreateDirectory::plan(NIX_CONF_FOLDER, "root".into(), "root".into(), 0o0755, force).await?;
        let create_file =
            CreateFile::plan(NIX_CONF, "root".into(), "root".into(), 0o0664, buf, force).await?;
        Ok(Self { create_directory, create_file })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for PlaceNixConfiguration {
    type Receipt = PlaceNixConfigurationReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Place the nix configuration in `{NIX_CONF}`"),
            vec!["This file is read by the Nix daemon to set its configuration options at runtime.".to_string()],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { create_file, create_directory } = self;
        let create_directory = create_directory.execute().await?;
        let create_file = create_file.execute().await?;
        Ok(Self::Receipt { create_file, create_directory })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceNixConfigurationReceipt {
    create_directory: CreateDirectoryReceipt,
    create_file: CreateFileReceipt,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for PlaceNixConfigurationReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
