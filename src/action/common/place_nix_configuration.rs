use tracing::{span, Span};

use crate::action::base::create_or_merge_nix_config::CreateOrMergeNixConfigError;
use crate::action::base::{CreateDirectory, CreateOrMergeNixConfig};
use crate::action::{Action, ActionDescription, ActionError, StatefulAction};

const NIX_CONF_FOLDER: &str = "/etc/nix";
const NIX_CONF: &str = "/etc/nix/nix.conf";

/**
Place the `/etc/nix.conf` file
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceNixConfiguration {
    create_directory: StatefulAction<CreateDirectory>,
    create_or_merge_nix_config: StatefulAction<CreateOrMergeNixConfig>,
}

impl PlaceNixConfiguration {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        nix_build_group_name: String,
        extra_conf: Vec<String>,
        force: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let extra_conf = extra_conf.join("\n");
        let mut nix_config = nix_config_parser::parse_nix_config_string(extra_conf, None)
            .map_err(CreateOrMergeNixConfigError::ParseNixConfig)
            .map_err(|e| ActionError::Custom(Box::new(e)))?;
        let settings = nix_config.settings_mut();

        settings.insert("build-users-group".into(), nix_build_group_name.into());
        settings.insert("experimental-features".into(), "nix-command flakes".into());
        settings.insert("auto-optimise-store".into(), "true".into());
        settings.insert("bash-prompt-prefix".into(), "(nix:$name)\\040".into());
        settings.insert("extra-nix-path".into(), "nixpkgs=flake:nixpkgs".into());

        let create_directory =
            CreateDirectory::plan(NIX_CONF_FOLDER, None, None, 0o0755, force).await?;
        let create_or_merge_nix_config = CreateOrMergeNixConfig::plan(NIX_CONF, nix_config).await?;
        Ok(Self {
            create_directory,
            create_or_merge_nix_config,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "place_nix_configuration")]
impl Action for PlaceNixConfiguration {
    fn tracing_synopsis(&self) -> String {
        format!("Place the Nix configuration in `{NIX_CONF}`")
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "place_nix_configuration",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![
                "This file is read by the Nix daemon to set its configuration options at runtime."
                    .to_string(),
            ],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            create_or_merge_nix_config,
            create_directory,
        } = self;

        create_directory.try_execute().await?;
        create_or_merge_nix_config.try_execute().await?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Remove the Nix configuration in `{NIX_CONF}`"),
            vec![
                "This file is read by the Nix daemon to set its configuration options at runtime."
                    .to_string(),
            ],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            create_or_merge_nix_config,
            create_directory,
        } = self;

        create_or_merge_nix_config.try_revert().await?;
        create_directory.try_revert().await?;

        Ok(())
    }
}
