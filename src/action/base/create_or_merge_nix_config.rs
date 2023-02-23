use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use nix_config_parser::NixConfig;
use rand::Rng;
use tokio::{
    fs::{remove_file, OpenOptions},
    io::AsyncWriteExt,
};
use tracing::{span, Span};

use crate::action::{Action, ActionDescription, ActionError, StatefulAction};

/// The `nix.conf` configuration names that are safe to merge.
// FIXME(@cole-h): make configurable by downstream users?
const MERGEABLE_CONF_NAMES: &[&str] = &["experimental-features"];
const NIX_CONF_MODE: u32 = 0o644;

/// Create or merge an existing `nix.conf` at the specified path.
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateOrMergeNixConfig {
    pub(crate) path: PathBuf,
    nix_configs: NixConfigs,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
struct NixConfigs {
    existing_nix_config: Option<NixConfig>,
    merged_nix_config: NixConfig,
}

impl CreateOrMergeNixConfig {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        buf: String,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref().to_path_buf();

        let pending_nix_config = nix_config_parser::parse_nix_config_string(buf.clone(), None)
            .map_err(ActionError::ParseNixConfig)?;
        let nix_configs = NixConfigs {
            existing_nix_config: None,
            merged_nix_config: NixConfig::new(),
        };

        let mut this = Self { path, nix_configs };

        if this.path.exists() {
            let metadata = this
                .path
                .metadata()
                .map_err(|e| ActionError::GettingMetadata(this.path.clone(), e))?;

            if !metadata.is_file() {
                return Err(ActionError::PathWasNotFile(this.path));
            }

            // Does the file have the right permissions?
            let discovered_mode = metadata.permissions().mode();
            // We only care about user-group-other permissions
            let discovered_mode = discovered_mode & 0o777;

            if discovered_mode != NIX_CONF_MODE {
                return Err(ActionError::PathModeMismatch(
                    this.path,
                    discovered_mode,
                    NIX_CONF_MODE,
                ));
            }

            let existing_nix_config = nix_config_parser::parse_nix_config_file(&this.path)
                .map_err(ActionError::ParseNixConfig)?;
            this.nix_configs.existing_nix_config = Some(existing_nix_config.clone());
            let mut merged_nix_config = NixConfig::new();
            let mut unmergeable_config_names = Vec::new();

            for (pending_conf_name, pending_conf_value) in &pending_nix_config {
                if let Some(existing_conf_value) = existing_nix_config.get(pending_conf_name) {
                    let pending_conf_value = pending_conf_value.split(' ').collect::<Vec<_>>();
                    let existing_conf_value = existing_conf_value.split(' ').collect::<Vec<_>>();

                    if pending_conf_value
                        .iter()
                        .all(|e| existing_conf_value.contains(e))
                    {
                        // If _all_ the values we want are present in the existing config,
                        // merged_nix_config will be empty and this will be marked as completed. We
                        // don't return early here because there may be more config options to
                        // check.
                    } else if MERGEABLE_CONF_NAMES.contains(&pending_conf_name.as_str()) {
                        let pending_conf_value = pending_conf_value.join(" ");
                        let existing_conf_value = existing_conf_value.join(" ");

                        merged_nix_config.insert(
                            pending_conf_name.to_owned(),
                            format!("{pending_conf_value} {existing_conf_value}"),
                        );
                    } else {
                        unmergeable_config_names.push(pending_conf_name.to_owned());
                    }
                } else {
                    merged_nix_config
                        .insert(pending_conf_name.to_owned(), pending_conf_value.to_owned());
                }
            }

            if !unmergeable_config_names.is_empty() {
                return Err(ActionError::UnmergeableConfig(
                    unmergeable_config_names,
                    this.path,
                ));
            }

            if !merged_nix_config.is_empty() {
                this.nix_configs.merged_nix_config = merged_nix_config;
                return Ok(StatefulAction::uncompleted(this));
            }

            tracing::debug!(
                "File `{}` already contains what we want",
                this.path.display()
            );
            return Ok(StatefulAction::completed(this));
        } else {
            let mut merged_nix_config: NixConfig = NixConfig::new();
            for (pending_conf_name, pending_conf_value) in &pending_nix_config {
                merged_nix_config
                    .insert(pending_conf_name.to_owned(), pending_conf_value.to_owned());
            }

            if !merged_nix_config.is_empty() {
                this.nix_configs.merged_nix_config = merged_nix_config;
            }
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_or_merge_nix_config")]
impl Action for CreateOrMergeNixConfig {
    fn tracing_synopsis(&self) -> String {
        format!("Create or merge nix.conf file `{}`", self.path.display())
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_file",
            path = tracing::field::display(self.path.display()),
            mode = tracing::field::display(format!("{:#o}", NIX_CONF_MODE)),
            merged_nix_config = tracing::field::Empty,
        );

        if tracing::enabled!(tracing::Level::TRACE) {
            span.record(
                "merged_nix_config",
                &self
                    .nix_configs
                    .merged_nix_config
                    .iter()
                    .map(|(k, v)| format!("{k}='{v}'"))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        span
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!(
                "If {} already exists, we will attempt to merge the current settings with our settings; \
                otherwise, it will be created with only our settings",
                self.path.display()
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { path, nix_configs } = self;

        if tracing::enabled!(tracing::Level::TRACE) {
            let span = tracing::Span::current();
            span.record(
                "merged_nix_config",
                nix_configs
                    .merged_nix_config
                    .iter()
                    .map(|(k, v)| format!("{k}='{v}'"))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }

        let mut options = OpenOptions::new();
        options.create(true).write(true).read(true);
        options.mode(NIX_CONF_MODE);

        // Create a temporary file in the same directory as the one
        // that the final file goes in, so that we can rename it
        // atomically
        let parent_dir = path.parent().expect("File must be in a directory");
        let mut temp_file_path = parent_dir.to_owned();
        {
            let mut rng = rand::thread_rng();
            temp_file_path.push(format!("nix-installer-tmp.{}", rng.gen::<u32>()));
        }
        let mut temp_file = OpenOptions::new()
            .create(true)
            .write(true)
            // If the file is created, ensure that it has harmless
            // permissions regardless of whether the mode will be
            // changed later (if we ever create setuid executables,
            // they should only become setuid once they are owned by
            // the appropriate user)
            .mode(0o600)
            .open(&temp_file_path)
            .await
            .map_err(|e| {
                ActionError::Open(temp_file_path.clone(), e)
            })?;

        // FIXME(@cole-h): for now we replace the entire file, but in the future we could potentially "replace" the contents
        let mut new_config = String::new();
        if let Some(existing_nix_config) = &nix_configs.existing_nix_config {
            for (name, value) in existing_nix_config {
                if nix_configs.merged_nix_config.get(name).is_some() {
                    continue;
                }

                new_config.push_str(name);
                new_config.push_str(" = ");
                new_config.push_str(value);
                new_config.push('\n');
            }

            new_config.push('\n');
        }

        new_config.push_str(&format!(
            "# Generated by https://github.com/DeterminateSystems/nix-installer, version {version}.\n",
            version = env!("CARGO_PKG_VERSION"),
        ));

        for (name, value) in &nix_configs.merged_nix_config {
            new_config.push_str(name);
            new_config.push_str(" = ");
            new_config.push_str(value);
            new_config.push('\n');
        }

        temp_file
            .write_all(new_config.as_bytes())
            .await
            .map_err(|e| ActionError::Write(temp_file_path.clone(), e))?;

        tokio::fs::rename(&temp_file_path, &path)
            .await
            .map_err(|e| ActionError::Rename(path.to_owned(), temp_file_path.to_owned(), e))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            nix_configs: _,
        } = &self;

        vec![ActionDescription::new(
            format!("Delete file `{}`", path.display()),
            vec![format!("Delete file `{}`", path.display())],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            nix_configs: _,
        } = self;

        remove_file(&path)
            .await
            .map_err(|e| ActionError::Remove(path.to_owned(), e))?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use eyre::eyre;
    use tokio::fs::write;

    #[tokio::test]
    async fn creates_and_deletes_file() -> eyre::Result<()> {
        let temp_dir = tempdir::TempDir::new("nix_installer_tests_create_file")?;
        let test_file = temp_dir.path().join("creates_and_deletes_file");
        let mut action = CreateOrMergeNixConfig::plan(
            &test_file,
            "experimental-features = ca-references".into(),
        )
        .await?;

        action.try_execute().await?;

        let s = std::fs::read_to_string(&test_file)?;
        assert!(s.contains("# Generated by"));
        assert!(s.contains("ca-references"));
        assert!(nix_config_parser::parse_nix_config_file(&test_file).is_ok());

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn creates_and_deletes_file_even_if_edited() -> eyre::Result<()> {
        let temp_dir = tempdir::TempDir::new("nix_installer_tests_create_file")?;
        let test_file = temp_dir
            .path()
            .join("creates_and_deletes_file_even_if_edited");
        let mut action = CreateOrMergeNixConfig::plan(
            &test_file,
            "experimental-features = ca-references".into(),
        )
        .await?;

        action.try_execute().await?;

        write(test_file.as_path(), "More content").await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_exact_files_and_reverts_them() -> eyre::Result<()> {
        let temp_dir = tempdir::TempDir::new("nix_installer_tests_create_file")?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_exact_files_and_reverts_them");

        let test_content = "experimental-features = flakes";
        write(test_file.as_path(), test_content).await?;
        tokio::fs::set_permissions(&test_file, PermissionsExt::from_mode(NIX_CONF_MODE)).await?;

        let mut action = CreateOrMergeNixConfig::plan(&test_file, test_content.into()).await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_different_files_and_merges() -> eyre::Result<()> {
        let temp_dir = tempdir::TempDir::new("nix_installer_tests_create_file")?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_different_files_and_merges");

        write(
            test_file.as_path(),
            "experimental-features = flakes\nwarn-dirty = true\n",
        )
        .await?;
        tokio::fs::set_permissions(&test_file, PermissionsExt::from_mode(NIX_CONF_MODE)).await?;

        let mut action = CreateOrMergeNixConfig::plan(
            &test_file,
            "experimental-features = nix-command flakes\nallow-dirty = false\n".into(),
        )
        .await?;

        action.try_execute().await?;

        let s = std::fs::read_to_string(&test_file)?;
        assert!(s.contains("# Generated by"));
        assert!(s.contains("flakes"));
        assert!(s.contains("nix-command"));
        assert!(s.contains("allow-dirty = false"));
        assert!(s.contains("warn-dirty = true"));
        assert!(nix_config_parser::parse_nix_config_file(&test_file).is_ok());

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_different_files_and_fails_to_merge() -> eyre::Result<()> {
        let temp_dir = tempdir::TempDir::new("nix_installer_tests_create_file")?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_different_files_and_fails_to_merge");

        write(
            test_file.as_path(),
            "experimental-features = flakes\nwarn-dirty = true\n",
        )
        .await?;

        match CreateOrMergeNixConfig::plan(
            &test_file,
            "experimental-features = nix-command flakes\nwarn-dirty = false\n".into(),
        )
        .await
        {
            Err(ActionError::UnmergeableConfig(_, path)) => assert_eq!(path, test_file.as_path()),
            _ => return Err(eyre!("Should have returned ActionError::UnmergeableConfig")),
        }

        assert!(test_file.exists(), "File should not have been deleted");

        Ok(())
    }
}
