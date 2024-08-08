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

use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

/// The `nix.conf` configuration names that are safe to merge.
// FIXME(@cole-h): make configurable by downstream users?
const MERGEABLE_CONF_NAMES: &[&str] = &["experimental-features"];
const NIX_CONF_MODE: u32 = 0o664;
const NIX_CONF_COMMENT_CHAR: char = '#';

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum CreateOrMergeNixConfigError {
    #[error(transparent)]
    ParseNixConfig(#[from] nix_config_parser::ParseError),
    #[error("Could not merge Nix configuration for key(s) {}; consider removing them from `{1}` in your editor, or removing your existing configuration with `rm {1}`",
        .0
        .iter()
        .map(|v| format!("`{v}`"))
        .collect::<Vec<_>>()
        .join(", "))]
    UnmergeableConfig(Vec<String>, std::path::PathBuf),
}

impl From<CreateOrMergeNixConfigError> for ActionErrorKind {
    fn from(val: CreateOrMergeNixConfigError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}

/// Create or merge an existing `nix.conf` at the specified path.
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_or_merge_nix_config")]
pub struct CreateOrMergeNixConfig {
    pub(crate) path: PathBuf,
    pending_nix_config: NixConfig,
}

impl CreateOrMergeNixConfig {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        pending_nix_config: NixConfig,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref().to_path_buf();

        let this = Self {
            path,
            pending_nix_config,
        };

        if this.path.exists() {
            let (merged_nix_config, _) =
                Self::validate_existing_nix_config(&this.pending_nix_config, &this.path)?;

            if !merged_nix_config.settings().is_empty() {
                return Ok(StatefulAction::uncompleted(this));
            } else {
                tracing::debug!(
                    "Setting Nix configurations in `{}` already complete",
                    this.path.display()
                );
                return Ok(StatefulAction::completed(this));
            }
        }

        Ok(StatefulAction::uncompleted(this))
    }

    fn merge_pending_and_existing_nix_config(
        pending_nix_config: &NixConfig,
        existing_nix_config: &NixConfig,
        path: &Path,
    ) -> Result<(NixConfig, NixConfig), CreateOrMergeNixConfigError> {
        let mut merged_nix_config = NixConfig::new();
        let mut unmergeable_config_names = Vec::new();

        for (pending_conf_name, pending_conf_value) in pending_nix_config.settings() {
            if let Some(existing_conf_value) = existing_nix_config.settings().get(pending_conf_name)
            {
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
                    let mut merged_conf_value =
                        Vec::with_capacity(pending_conf_value.len() + existing_conf_value.len());
                    merged_conf_value.extend(pending_conf_value);
                    merged_conf_value.extend(existing_conf_value);
                    merged_conf_value.dedup();
                    let merged_conf_value = merged_conf_value.join(" ");
                    let merged_conf_value = merged_conf_value.trim();

                    merged_nix_config
                        .settings_mut()
                        .insert(pending_conf_name.to_owned(), merged_conf_value.to_owned());
                } else {
                    unmergeable_config_names.push(pending_conf_name.to_owned());
                }
            } else {
                merged_nix_config
                    .settings_mut()
                    .insert(pending_conf_name.to_owned(), pending_conf_value.to_owned());
            }
        }

        if !unmergeable_config_names.is_empty() {
            return Err(CreateOrMergeNixConfigError::UnmergeableConfig(
                unmergeable_config_names,
                path.to_path_buf(),
            ));
        }

        Ok((merged_nix_config, existing_nix_config.clone()))
    }

    fn validate_existing_nix_config(
        pending_nix_config: &NixConfig,
        path: &Path,
    ) -> Result<(NixConfig, NixConfig), ActionError> {
        let path = path.to_path_buf();
        let metadata = path
            .metadata()
            .map_err(|e| Self::error(ActionErrorKind::GettingMetadata(path.clone(), e)))?;

        if !metadata.is_file() {
            return Err(Self::error(ActionErrorKind::PathWasNotFile(path)));
        }

        // Does the file have the right permissions?
        let discovered_mode = metadata.permissions().mode();
        // We only care about user-group-other permissions
        let discovered_mode = discovered_mode & 0o777;

        if discovered_mode != NIX_CONF_MODE {
            return Err(Self::error(ActionErrorKind::PathModeMismatch(
                path,
                discovered_mode,
                NIX_CONF_MODE,
            )));
        }

        let existing_nix_config = NixConfig::parse_file(&path)
            .map_err(CreateOrMergeNixConfigError::ParseNixConfig)
            .map_err(Self::error)?;

        let (merged_nix_config, existing_nix_config) = Self::merge_pending_and_existing_nix_config(
            pending_nix_config,
            &existing_nix_config,
            &path,
        )
        .map_err(Self::error)?;

        Ok((merged_nix_config, existing_nix_config))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_or_merge_nix_config")]
impl Action for CreateOrMergeNixConfig {
    fn action_tag() -> ActionTag {
        ActionTag("create_or_merge_nix_config")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Merge or create nix.conf file `{path}`",
            path = self.path.display(),
        )
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_or_merge_nix_config",
            path = tracing::field::display(self.path.display()),
            mode = tracing::field::display(format!("{:#o}", NIX_CONF_MODE)),
            pending_nix_config = tracing::field::Empty,
        );

        if tracing::enabled!(tracing::Level::TRACE) {
            span.record(
                "pending_nix_config",
                &self
                    .pending_nix_config
                    .settings()
                    .iter()
                    .map(|(k, v)| format!("{k}=\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        span
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!(
                "Added settings: {settings}",
                settings = self
                    .pending_nix_config
                    .settings()
                    .iter()
                    .map(|(k, v)| format!("{k}=\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(", "),
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            pending_nix_config,
        } = self;

        if tracing::enabled!(tracing::Level::TRACE) {
            let span = tracing::Span::current();
            span.record(
                "pending_nix_config",
                pending_nix_config
                    .settings()
                    .iter()
                    .map(|(k, v)| format!("{k}='{v}'"))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }

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
            .truncate(true)
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
                Self::error(ActionErrorKind::Open(temp_file_path.clone(), e))
            })?;

        let (mut merged_nix_config, mut existing_nix_config) = if path.exists() {
            let (merged_nix_config, existing_nix_config) =
                Self::validate_existing_nix_config(pending_nix_config, path)?;
            (merged_nix_config, Some(existing_nix_config))
        } else {
            (pending_nix_config.clone(), None)
        };

        let mut new_config = String::new();

        if let Some(existing_nix_config) = existing_nix_config.as_mut() {
            let mut discovered_buf = tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| Self::error(ActionErrorKind::Read(path.to_path_buf(), e)))?;

            // We append a newline to ensure that, in the case there are comments at the end of the
            // file and _NO_ trailing newline, we still preserve the entire block of comments.
            discovered_buf.push('\n');

            let (associated_lines, _, _) = discovered_buf.split('\n').fold(
                (Vec::new(), Vec::new(), false),
                |(mut all_assoc, mut current_assoc, mut associating): (
                    Vec<Vec<String>>,
                    Vec<String>,
                    bool,
                ),
                 line| {
                    let line = line.trim();

                    // Don't associate our "Generated by" comment if it appears
                    if line.starts_with("# Generated by") {
                        return (all_assoc, current_assoc, associating);
                    }

                    if line.starts_with(NIX_CONF_COMMENT_CHAR) {
                        associating = true;
                    } else if line.is_empty() || !line.starts_with(NIX_CONF_COMMENT_CHAR) {
                        associating = false;
                    }

                    current_assoc.push(line.to_string());

                    if !associating {
                        all_assoc.push(current_assoc);
                        current_assoc = Vec::new();
                    }

                    (all_assoc, current_assoc, associating)
                },
            );

            for line_group in associated_lines {
                if line_group.is_empty() || line_group.iter().all(|line| line.is_empty()) {
                    continue;
                }

                // This expect should never reasonably panic, because we would need a line group
                // consisting solely of a comment and nothing else, but unconditionally appending a
                // newline to the config string before grouping above prevents this from occurring.
                let line_idx = line_group
                    .iter()
                    .position(|line| !line.starts_with(NIX_CONF_COMMENT_CHAR))
                    .expect("There should always be one line without a comment character");

                let setting_line = &line_group[line_idx];
                let comments = line_group[..line_idx].join("\n");

                // If we're here, but the line without a comment char is empty, we have
                // standalone comments to preserve, but no settings with inline comments.
                if setting_line.is_empty() {
                    for line in &line_group {
                        new_config.push_str(line);
                        new_config.push('\n');
                    }

                    continue;
                }

                // Preserve inline comments for settings we've merged
                let to_remove = if let Some((name, value)) = existing_nix_config
                    .settings()
                    .iter()
                    .find(|(name, _value)| setting_line.starts_with(*name))
                {
                    let inline_comment_idx =
                        if let Some(idx) = setting_line.find(NIX_CONF_COMMENT_CHAR) {
                            idx
                        } else {
                            continue;
                        };

                    let inline_comment = &setting_line[inline_comment_idx..];

                    new_config.push_str(&comments);
                    new_config.push('\n');
                    new_config.push_str(name);
                    new_config.push_str(" = ");

                    if let Some(merged_value) = merged_nix_config.settings_mut().swap_remove(name) {
                        new_config.push_str(&merged_value);
                        new_config.push(' ');
                    } else {
                        new_config.push_str(value);
                    }

                    new_config.push_str(inline_comment);
                    new_config.push('\n');

                    Some(name.clone())
                } else {
                    new_config.push_str(&comments);
                    new_config.push('\n');
                    new_config.push_str(setting_line);
                    new_config.push('\n');

                    None
                };

                if let Some(to_remove) = to_remove {
                    existing_nix_config.settings_mut().swap_remove(&to_remove);
                }
            }

            // Add the leftover existing nix config
            for (name, value) in existing_nix_config.settings() {
                if merged_nix_config.settings().get(name).is_some() {
                    continue;
                }

                new_config.push_str(name);
                new_config.push_str(" = ");
                new_config.push_str(value);
                new_config.push('\n');
            }

            new_config.push('\n');
        }

        new_config
            .push_str("# Generated by https://github.com/DeterminateSystems/nix-installer.\n");
        new_config.push_str("# See `/nix/nix-installer --version` for the version details.\n");
        new_config.push('\n');

        for (name, value) in merged_nix_config.settings() {
            new_config.push_str(name);
            new_config.push_str(" = ");
            new_config.push_str(value);
            new_config.push('\n');
        }

        temp_file
            .write_all(new_config.as_bytes())
            .await
            .map_err(|e| Self::error(ActionErrorKind::Write(temp_file_path.clone(), e)))?;
        tokio::fs::set_permissions(&temp_file_path, PermissionsExt::from_mode(NIX_CONF_MODE))
            .await
            .map_err(|e| {
                Self::error(ActionErrorKind::SetPermissions(
                    NIX_CONF_MODE,
                    path.to_owned(),
                    e,
                ))
            })?;
        temp_file
            .sync_all()
            .await
            .map_err(|e| Self::error(ActionErrorKind::Sync(temp_file_path.clone(), e)))?;
        tokio::fs::rename(&temp_file_path, &path)
            .await
            .map_err(|e| {
                Self::error(ActionErrorKind::Rename(
                    temp_file_path.to_owned(),
                    path.to_owned(),
                    e,
                ))
            })?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            pending_nix_config: _,
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
            pending_nix_config: _,
        } = self;

        remove_file(&path)
            .await
            .map_err(|e| Self::error(ActionErrorKind::Remove(path.to_owned(), e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use color_eyre::eyre::eyre;
    use tokio::fs::write;

    #[tokio::test]
    async fn creates_and_deletes_file() -> eyre::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let test_file = temp_dir.path().join("creates_and_deletes_file");
        let mut nix_config = NixConfig::new();
        nix_config
            .settings_mut()
            .insert("experimental-features".into(), "ca-references".into());
        let mut action = CreateOrMergeNixConfig::plan(&test_file, nix_config).await?;

        action.try_execute().await?;

        let s = std::fs::read_to_string(&test_file)?;
        assert!(s.contains("# Generated by"));
        assert!(s.contains("ca-references"));
        assert!(NixConfig::parse_file(&test_file).is_ok());

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn creates_and_deletes_file_even_if_edited() -> eyre::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let test_file = temp_dir
            .path()
            .join("creates_and_deletes_file_even_if_edited");
        let mut nix_config = NixConfig::new();
        nix_config
            .settings_mut()
            .insert("experimental-features".into(), "ca-references".into());
        let mut action = CreateOrMergeNixConfig::plan(&test_file, nix_config).await?;

        action.try_execute().await?;

        write(test_file.as_path(), "More content").await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_exact_files_and_reverts_them() -> eyre::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_exact_files_and_reverts_them");

        let test_content = "experimental-features = flakes";
        write(test_file.as_path(), test_content).await?;
        tokio::fs::set_permissions(&test_file, PermissionsExt::from_mode(NIX_CONF_MODE)).await?;

        let mut nix_config = NixConfig::new();
        nix_config
            .settings_mut()
            .insert("experimental-features".into(), "flakes".into());
        let mut action = CreateOrMergeNixConfig::plan(&test_file, nix_config).await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_different_files_and_merges() -> eyre::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_different_files_and_merges");

        write(
            test_file.as_path(),
            "experimental-features = flakes\nwarn-dirty = true\n",
        )
        .await?;
        tokio::fs::set_permissions(&test_file, PermissionsExt::from_mode(NIX_CONF_MODE)).await?;

        let mut nix_config = NixConfig::new();
        nix_config
            .settings_mut()
            .insert("experimental-features".into(), "nix-command flakes".into());
        nix_config
            .settings_mut()
            .insert("allow-dirty".into(), "false".into());
        let mut action = CreateOrMergeNixConfig::plan(&test_file, nix_config).await?;

        action.try_execute().await?;

        let s = std::fs::read_to_string(&test_file)?;
        assert!(s.contains("# Generated by"));
        assert!(s.contains("flakes"));
        assert!(s.contains("nix-command"));
        assert_eq!(
            s.matches("flakes").count(),
            1,
            "we should not duplicate strings"
        );
        assert!(s.contains("allow-dirty = false"));
        assert!(s.contains("warn-dirty = true"));
        assert!(NixConfig::parse_file(&test_file).is_ok());

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_different_files_and_fails_to_merge() -> eyre::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_different_files_and_fails_to_merge");

        write(
            test_file.as_path(),
            "experimental-features = flakes\nwarn-dirty = true\n",
        )
        .await?;
        tokio::fs::set_permissions(&test_file, PermissionsExt::from_mode(NIX_CONF_MODE)).await?;

        let mut nix_config = NixConfig::new();
        nix_config
            .settings_mut()
            .insert("experimental-features".into(), "nix-command flakes".into());
        nix_config
            .settings_mut()
            .insert("warn-dirty".into(), "false".into());
        match CreateOrMergeNixConfig::plan(&test_file, nix_config).await {
            Err(err) => {
                if let ActionErrorKind::Custom(e) = err.kind() {
                    match e.downcast_ref::<CreateOrMergeNixConfigError>() {
                        Some(CreateOrMergeNixConfigError::UnmergeableConfig(_, path)) => {
                            assert_eq!(path, test_file.as_path())
                        },
                        _ => {
                            return Err(eyre!(
                        "Should have returned CreateOrMergeNixConfigError::UnmergeableConfig"
                    ))
                        },
                    }
                }
            },
            _ => {
                return Err(eyre!(
                    "Should have returned CreateOrMergeNixConfigError::UnmergeableConfig"
                ))
            },
        }

        assert!(test_file.exists(), "File should not have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn preserves_comments() -> eyre::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let test_file = temp_dir.path().join("preserves_comments");

        write(
            test_file.as_path(),
            "# test 2\n# test\nexperimental-features = flakes # some inline comment about experimental-features\n# the following line should be warn-dirty = true\nwarn-dirty = true # this is an inline comment\n# this is an ungrouped comment\n# this too",
        )
        .await?;
        tokio::fs::set_permissions(&test_file, PermissionsExt::from_mode(NIX_CONF_MODE)).await?;
        let mut nix_config = NixConfig::new();
        nix_config
            .settings_mut()
            .insert("experimental-features".into(), "ca-references".into());
        let mut action = CreateOrMergeNixConfig::plan(&test_file, nix_config).await?;

        action.try_execute().await?;

        let s = std::fs::read_to_string(&test_file)?;
        assert!(s.contains("# the following line should be warn-dirty = true\nwarn-dirty = true"));
        assert!(s.contains("# test 2\n# test\nexperimental-features"));
        assert!(s.contains("# this is an inline comment"));
        assert!(s.contains("# some inline comment about experimental-features"));
        assert!(s.contains("# Generated by"));
        assert!(s.contains("# this is an ungrouped comment\n# this too"));
        assert!(s.contains("ca-references"));
        assert!(NixConfig::parse_file(&test_file).is_ok());

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn preserves_comments_edge_case() -> eyre::Result<()> {
        let temp_dir = tempfile::TempDir::new()?;
        let test_file = temp_dir.path().join("preserves_comments");

        write(test_file.as_path(), " a = b\n c = d# lol\n# e = f").await?;
        tokio::fs::set_permissions(&test_file, PermissionsExt::from_mode(NIX_CONF_MODE)).await?;
        let mut nix_config = NixConfig::new();
        nix_config
            .settings_mut()
            .insert("experimental-features".into(), "ca-references".into());
        let mut action = CreateOrMergeNixConfig::plan(&test_file, nix_config).await?;

        action.try_execute().await?;

        let s = std::fs::read_to_string(&test_file)?;
        assert!(s.contains("# Generated by"));
        assert!(s.contains("ca-references"));
        assert_eq!(s.matches("a = b").count(), 1);
        assert!(NixConfig::parse_file(&test_file).is_ok());

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }
}
