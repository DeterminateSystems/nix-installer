use tracing::{span, Span};

use crate::action::base::create_or_merge_nix_config::CreateOrMergeNixConfigError;
use crate::action::base::{CreateDirectory, CreateOrMergeNixConfig};
use crate::action::{Action, ActionDescription, ActionError, ActionTag, StatefulAction};

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
        let mut nix_config = nix_config_parser::NixConfig::parse_string(extra_conf, None)
            .map_err(CreateOrMergeNixConfigError::ParseNixConfig)
            .map_err(|e| ActionError::Custom(Box::new(e)))?;
        let settings = nix_config.settings_mut();

        settings.insert("build-users-group".to_string(), nix_build_group_name);
        settings.insert(
            "experimental-features".to_string(),
            "nix-command flakes".to_string(),
        );
        settings.insert("auto-optimise-store".to_string(), "true".to_string());
        settings.insert(
            "bash-prompt-prefix".to_string(),
            "(nix:$name)\\040".to_string(),
        );
        settings.insert(
            "extra-nix-path".to_string(),
            "nixpkgs=flake:nixpkgs".to_string(),
        );

        let create_directory = CreateDirectory::plan(NIX_CONF_FOLDER, None, None, 0o0755, force)
            .await
            .map_err(|e| ActionError::Child(CreateDirectory::action_tag(), Box::new(e)))?;
        let create_or_merge_nix_config = CreateOrMergeNixConfig::plan(NIX_CONF, nix_config)
            .await
            .map_err(|e| {
            ActionError::Child(CreateOrMergeNixConfig::action_tag(), Box::new(e))
        })?;
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
    fn action_tag() -> ActionTag {
        ActionTag("place_nix_configuration")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Place the Nix configuration in `{NIX_CONF}`")
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "place_nix_configuration",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let Self {
            create_or_merge_nix_config,
            create_directory,
        } = self;

        let mut explanation = vec![
            "This file is read by the Nix daemon to set its configuration options at runtime."
                .to_string(),
        ];

        if let Some(val) = create_directory.describe_execute().iter().next() {
            explanation.push(val.description.clone())
        }
        for val in create_or_merge_nix_config.describe_execute().iter() {
            explanation.push(val.description.clone())
        }

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        self.create_directory
            .try_execute()
            .await
            .map_err(|e| ActionError::Child(self.create_directory.action_tag(), Box::new(e)))?;
        self.create_or_merge_nix_config
            .try_execute()
            .await
            .map_err(|e| {
                ActionError::Child(self.create_or_merge_nix_config.action_tag(), Box::new(e))
            })?;

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
    async fn revert(&mut self) -> Result<(), Vec<ActionError>> {
        let mut errors = vec![];
        if let Err(err) = self
            .create_or_merge_nix_config
            .try_revert()
            .await
            .map_err(|errs| {
                ActionError::ChildRevert(self.create_or_merge_nix_config.action_tag(), errs)
            })
        {
            errors.push(err);
        }
        if let Err(err) = self
            .create_directory
            .try_revert()
            .await
            .map_err(|errs| ActionError::ChildRevert(self.create_directory.action_tag(), errs))
        {
            errors.push(err);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
