use tracing::{span, Span};

use crate::action::base::create_or_insert_into_file::Position;
use crate::action::base::{CreateDirectory, CreateFile, CreateOrInsertIntoFile};
use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};
use std::path::{Path, PathBuf};

const NIX_CONF_FOLDER: &str = "/etc/nix";
const NIX_CONF: &str = "/etc/nix/nix.conf";

/**
Place the `/etc/nix.conf` file
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct PlaceNixConfiguration {
    create_directory: StatefulAction<CreateDirectory>,
    create_or_insert_nix_conf: StatefulAction<CreateOrInsertIntoFile>,
    create_defaults_conf: StatefulAction<CreateFile>,
}

impl PlaceNixConfiguration {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        nix_build_group_name: String,
        ssl_cert_file: Option<PathBuf>,
        extra_conf: Vec<String>,
        force: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let nix_conf_insert_position = if Path::new(NIX_CONF).exists() {
            let existing_nix_conf = tokio::fs::read_to_string(NIX_CONF)
                .await
                .map_err(|e| ActionErrorKind::Read(NIX_CONF.into(), e))
                .map_err(Self::error)?;
            // Find the first line that isn't just `# ...` comments
            let existing_nix_conf_lines = existing_nix_conf.lines();
            let chosen_insert_after = existing_nix_conf_lines
                .enumerate()
                .find_map(|(index, value)| {
                    if value.starts_with("#") {
                        None
                    } else {
                        Some((index, value.to_string()))
                    }
                })
                .map(|(index, expected_content)| Position::Before {
                    index,
                    expected_content,
                });
            // If `None` then the file is likely just completely empty.
            chosen_insert_after.unwrap_or(Position::Beginning)
        } else {
            Position::Beginning
        };

        let mut nix_conf_insert_settings = Vec::default();
        nix_conf_insert_settings.push("!include ./defaults.conf".into());
        nix_conf_insert_settings.extend(extra_conf);
        let nix_conf_insert_fragment = nix_conf_insert_settings.join("\n");

        let mut defaults_conf_settings = Vec::default();
        defaults_conf_settings.push(format!("build-users-group = {nix_build_group_name}"));
        defaults_conf_settings
            .push("experimental-features = nix-command flakes auto-allocate-uids".into());
        // https://github.com/DeterminateSystems/nix-installer/issues/449#issuecomment-1551782281
        defaults_conf_settings.push("bash-prompt-prefix = (nix:$name)\\040".into());
        defaults_conf_settings.push("extra-nix-path = nixpkgs=flake:nixpkgs".into());
        if let Some(ssl_cert_file) = ssl_cert_file {
            let ssl_cert_file_canonical = ssl_cert_file
                .canonicalize()
                .map_err(|e| Self::error(ActionErrorKind::Canonicalize(ssl_cert_file, e)))?;
            defaults_conf_settings.push(format!(
                "ssl-cert-file = {}",
                ssl_cert_file_canonical.display()
            ));
        }
        #[cfg(not(target_os = "macos"))]
        defaults_conf_settings.push("auto-optimise-store = true".into());
        // Auto-allocate uids is broken on Mac. Tools like `whoami` don't work.
        // e.g. https://github.com/NixOS/nix/issues/8444
        #[cfg(not(target_os = "macos"))]
        defaults_conf_settings.push("auto-allocate-uids = true".into());
        let defaults_conf_insert_fragment = defaults_conf_settings.join("\n");

        let create_directory = CreateDirectory::plan(NIX_CONF_FOLDER, None, None, 0o0755, force)
            .await
            .map_err(Self::error)?;

        let create_or_insert_nix_conf = CreateOrInsertIntoFile::plan(
            NIX_CONF,
            None,
            None,
            0o755,
            nix_conf_insert_fragment + "\n",
            nix_conf_insert_position,
        )
        .await
        .map_err(Self::error)?;

        let create_defaults_conf = CreateFile::plan(
            PathBuf::from("/etc/nix/defaults.conf"),
            None,
            None,
            0o755,
            defaults_conf_insert_fragment + "\n",
            true,
        )
        .await
        .map_err(Self::error)?;

        Ok(Self {
            create_directory,
            create_or_insert_nix_conf,
            create_defaults_conf,
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
            create_directory,
            create_or_insert_nix_conf,
            create_defaults_conf,
        } = self;

        let mut explanation = vec![
            "This file is read by the Nix daemon to set its configuration options at runtime."
                .to_string(),
        ];

        if let Some(val) = create_directory.describe_execute().first() {
            explanation.push(val.description.clone())
        }
        for val in create_or_insert_nix_conf.describe_execute().iter() {
            explanation.push(val.description.clone())
        }
        for val in create_defaults_conf.describe_execute().iter() {
            explanation.push(val.description.clone())
        }

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        self.create_directory
            .try_execute()
            .await
            .map_err(Self::error)?;
        self.create_or_insert_nix_conf
            .try_execute()
            .await
            .map_err(Self::error)?;
        self.create_defaults_conf
            .try_execute()
            .await
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            create_directory,
            create_or_insert_nix_conf,
            create_defaults_conf,
        } = self;

        let mut explanation = vec![
            "This file is read by the Nix daemon to set its configuration options at runtime."
                .to_string(),
        ];

        if let Some(val) = create_directory.describe_execute().first() {
            explanation.push(val.description.clone())
        }
        for val in create_or_insert_nix_conf.describe_execute().iter() {
            explanation.push(val.description.clone())
        }
        for val in create_defaults_conf.describe_execute().iter() {
            explanation.push(val.description.clone())
        }

        vec![ActionDescription::new(
            format!("Remove the Nix configuration in `{NIX_CONF}`"),
            explanation,
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];
        if let Err(err) = self.create_or_insert_nix_conf.try_revert().await {
            errors.push(err);
        }
        if let Err(err) = self.create_defaults_conf.try_revert().await {
            errors.push(err);
        }
        if let Err(err) = self.create_directory.try_revert().await {
            errors.push(err);
        }

        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(errors
                .into_iter()
                .next()
                .expect("Expected 1 len Vec to have at least 1 item"))
        } else {
            Err(Self::error(ActionErrorKind::MultipleChildren(errors)))
        }
    }
}
