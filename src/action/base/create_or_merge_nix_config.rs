use nix::unistd::{chown, Group, User};
use nix_config_parser::NixConfig;
use tracing::{span, Span};

use std::{
    os::{unix::fs::MetadataExt, unix::fs::PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt},
};

use crate::action::{Action, ActionDescription, ActionError, StatefulAction};

const MERGEABLE_CONF_NAMES: &[&str] = &["experimental-features"];

/** Create a file at the given location with the provided `buf`,
optionally with an owning user, group, and mode.

If `force` is set, the file will always be overwritten (and deleted)
regardless of its presence prior to install.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateOrMergeNixConfig {
    pub(crate) path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    buf: String,
    nix_configs: NixConfigs,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
struct NixConfigs {
    pending_nix_config: NixConfig,
    existing_nix_config: Option<NixConfig>,
    merged_nix_config: Option<NixConfig>,
}

impl CreateOrMergeNixConfig {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        buf: String,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref().to_path_buf();
        let mode = mode.into();
        let user = user.into();
        let group = group.into();
        let pending_nix_config =
            nix_config_parser::parse_nix_config_string(buf.clone(), &Path::new("/"))
                .map_err(ActionError::ParseNixConfig)?;
        let nix_configs = NixConfigs {
            pending_nix_config,
            existing_nix_config: None,
            merged_nix_config: None,
        };

        let mut this = Self {
            path,
            user,
            group,
            mode,
            buf,
            nix_configs,
        };

        if this.path.exists() {
            let metadata = this
                .path
                .metadata()
                .map_err(|e| ActionError::GettingMetadata(this.path.clone(), e))?;

            if let Some(mode) = mode {
                // Does the file have the right permissions?
                let discovered_mode = metadata.permissions().mode();
                if discovered_mode != mode {
                    return Err(ActionError::PathModeMismatch(
                        this.path.clone(),
                        discovered_mode,
                        mode,
                    ));
                }
            }

            // Does it have the right user/group?
            if let Some(user) = &this.user {
                // If the file exists, the user must also exist to be correct.
                let expected_uid = User::from_name(user.as_str())
                    .map_err(|e| ActionError::GettingUserId(user.clone(), e))?
                    .ok_or_else(|| ActionError::NoUser(user.clone()))?
                    .uid;
                let found_uid = metadata.uid();
                if found_uid != expected_uid.as_raw() {
                    return Err(ActionError::PathUserMismatch(
                        this.path.clone(),
                        found_uid,
                        expected_uid.as_raw(),
                    ));
                }
            }
            if let Some(group) = &this.group {
                // If the file exists, the group must also exist to be correct.
                let expected_gid = Group::from_name(group.as_str())
                    .map_err(|e| ActionError::GettingGroupId(group.clone(), e))?
                    .ok_or_else(|| ActionError::NoUser(group.clone()))?
                    .gid;
                let found_gid = metadata.gid();
                if found_gid != expected_gid.as_raw() {
                    return Err(ActionError::PathGroupMismatch(
                        this.path.clone(),
                        found_gid,
                        expected_gid.as_raw(),
                    ));
                }
            }

            let existing_nix_config = nix_config_parser::parse_nix_config_file(&this.path)
                .map_err(ActionError::ParseNixConfig)?;
            let mut merged_nix_config: NixConfig = NixConfig::new();
            let mut unmergeable_config_names = Vec::new();
            this.nix_configs.existing_nix_config = Some(existing_nix_config.clone());

            for (pending_conf_name, pending_conf_value) in &this.nix_configs.pending_nix_config {
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
                    this.path.clone(),
                ));
            }

            if !merged_nix_config.is_empty() {
                this.nix_configs.merged_nix_config = Some(merged_nix_config);
                return Ok(StatefulAction::uncompleted(this));
            }

            tracing::debug!(
                "File `{}` already contains what we want",
                this.path.display()
            );
            return Ok(StatefulAction::completed(this));
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_or_merge_nix_config")]
impl Action for CreateOrMergeNixConfig {
    fn tracing_synopsis(&self) -> String {
        format!("Create or overwrite file `{}`", self.path.display())
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_file",
            path = tracing::field::display(self.path.display()),
            user = self.user,
            group = self.group,
            mode = self
                .mode
                .map(|v| tracing::field::display(format!("{:#o}", v))),
            buf = tracing::field::Empty,
        );

        if tracing::enabled!(tracing::Level::TRACE) {
            span.record("buf", &self.buf);
        }
        span
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
            nix_configs,
        } = self;

        if tracing::enabled!(tracing::Level::TRACE) {
            let span = tracing::Span::current();
            span.record("buf", &buf);
        }

        let mut options = OpenOptions::new();
        options.create(true).write(true).read(true);

        if let Some(mode) = mode {
            options.mode(*mode);
        }

        let mut file = options
            .open(&path)
            .await
            .map_err(|e| ActionError::Open(path.to_owned(), e))?;

        if let Some(merged_nix_config) = &nix_configs.merged_nix_config {
            // FIXME(@cole-h): for now we replace the entire file, but in the future we could potentially "replace" the contents
            let mut new_config = String::new();
            for (name, value) in nix_configs.existing_nix_config.as_ref().unwrap() {
                if merged_nix_config.get(name).is_some() {
                    continue;
                }

                new_config.push_str(name);
                new_config.push_str(" = ");
                new_config.push_str(value);
                new_config.push_str("\n");
            }

            new_config.push_str("\n");
            new_config.push_str(&format!(
                    "# Generated by https://github.com/DeterminateSystems/nix-installer, version {version}.\n",
                    version = env!("CARGO_PKG_VERSION"),
                ));

            for (name, value) in merged_nix_config {
                new_config.push_str(name);
                new_config.push_str(" = ");
                new_config.push_str(value);
                new_config.push_str("\n");
            }

            file.rewind()
                .await
                .map_err(|e| ActionError::Seek(path.to_owned(), e))?;
            file.set_len(0)
                .await
                .map_err(|e| ActionError::Truncate(path.to_owned(), e))?;
            file.write_all(new_config.as_bytes())
                .await
                .map_err(|e| ActionError::Write(path.to_owned(), e))?;
            file.flush()
                .await
                .map_err(|e| ActionError::Flush(path.to_owned(), e))?;
        }

        let gid = if let Some(group) = group {
            Some(
                Group::from_name(group.as_str())
                    .map_err(|e| ActionError::GettingGroupId(group.clone(), e))?
                    .ok_or(ActionError::NoGroup(group.clone()))?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| ActionError::GettingUserId(user.clone(), e))?
                    .ok_or(ActionError::NoUser(user.clone()))?
                    .uid,
            )
        } else {
            None
        };
        chown(path, uid, gid).map_err(|e| ActionError::Chown(path.clone(), e))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf: _,
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
            user: _,
            group: _,
            mode: _,
            buf: _,
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
            test_file.clone(),
            None,
            None,
            None,
            "experimental-features = ca-references".into(),
        )
        .await?;

        action.try_execute().await?;

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
            test_file.clone(),
            None,
            None,
            None,
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

        let mut action =
            CreateOrMergeNixConfig::plan(test_file.clone(), None, None, None, test_content.into())
                .await?;

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

        let mut action = CreateOrMergeNixConfig::plan(
            test_file.clone(),
            None,
            None,
            None,
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
            test_file.clone(),
            None,
            None,
            None,
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
