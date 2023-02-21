use nix::unistd::{chown, Group, User};
use tracing::{span, Span};

use std::{
    os::{unix::fs::MetadataExt, unix::fs::PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::{
    fs::{remove_file, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::action::{Action, ActionDescription, ActionError, StatefulAction};

/** Create a file at the given location with the provided `buf`,
optionally with an owning user, group, and mode.

If `force` is set, the file will always be overwritten (and deleted)
regardless of its presence prior to install.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateOrMergeNixConf {
    pub(crate) path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    buf: String,
}

impl CreateOrMergeNixConf {
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
        let this = Self {
            path,
            user,
            group,
            mode,
            buf,
        };

        if this.path.exists() {
            // If the path exists, perhaps we can just skip this
            let mut file = File::open(&this.path)
                .await
                .map_err(|e| ActionError::Open(this.path.clone(), e))?;

            let metadata = file
                .metadata()
                .await
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

            // TODO: CreateOrMergeNixConf
            // based off of CreateFile
            // uses nix-conf-parser to see if all settings in `buf` are already set
            //   * if so just skip?
            //   * if they don't, use hashmap funsies to merge the values that can
            //     * error if there are values that differ that can't be merged
            let existing_nix_config = nix_config_parser::parse_nix_config_file(&this.path)
                .expect("TODO: convert to ActionError");
            // TODO: Option<Path>
            let pending_nix_config =
                nix_config_parser::parse_nix_config_string(this.buf.clone(), &Path::new("/"))
                    .expect("TODO: convert to ActionError");
            let mut merged_nix_config: nix_config_parser::NixConfig =
                nix_config_parser::NixConfig::new();

            for (pending_conf_name, pending_conf_value) in &pending_nix_config {
                if let Some(existing_conf_value) = existing_nix_config.get(pending_conf_name) {
                    let pending_conf_value = pending_conf_value.split(' ').collect::<Vec<_>>();
                    let existing_conf_value = existing_conf_value.split(' ').collect::<Vec<_>>();

                    if !existing_conf_value
                        .iter()
                        .all(|e| pending_conf_value.contains(e))
                    {
                        const MERGABLE_CONF_NAMES: &'static [&str] = &["experimental-features"];
                        if MERGABLE_CONF_NAMES.contains(&pending_conf_name.as_str()) {
                            merged_nix_config.insert(
                                pending_conf_name.to_owned(),
                                format!(
                                    "{} {}",
                                    pending_conf_value.join(" "),
                                    existing_conf_value.join(" ")
                                ),
                            );
                        } else {
                            todo!("don't know how to merge {pending_conf_name}");
                        }
                    } else {
                        todo!("skipped because this one config has all values we wanted");
                    }
                }
            }

            if !merged_nix_config.is_empty() {
                // #[cfg(all())]
                #[cfg(any())]
                {
                    let mut discovered_buf = String::new();
                    file.read_to_string(&mut discovered_buf)
                        .await
                        .map_err(|e| ActionError::Read(this.path.clone(), e))?;

                    for (name, value) in &merged_nix_config {
                        let indices = discovered_buf.match_indices(name);
                    }
                }

                #[cfg(all())]
                // #[cfg(any())]
                {
                    let mut discovered_buf = String::new();
                    file.read_to_string(&mut discovered_buf)
                        .await
                        .map_err(|e| ActionError::Read(this.path.clone(), e))?;
                    let mut lines = discovered_buf.lines();

                    for (name, value) in &merged_nix_config {
                        todo!("see if any lines start with name; if so, replace line up to #");
                    }
                }

                #[cfg(any())]
                {
                    // FIXME(@cole-h): for now we replace the entire file, but in the future we could potentially "replace" the contents
                    let mut new_config = format!(
                    "# Generated by https://github.com/DeterminateSystems/nix-installer, version {version}.\n",
                    version = env!("CARGO_PKG_VERSION"),
                );
                    for (name, value) in &merged_nix_config {
                        new_config.push_str(name);
                        new_config.push_str(" = ");
                        new_config.push_str(value);
                        new_config.push_str("\n");
                    }
                    file.write_all(&new_config.as_bytes())
                        .await
                        .expect("TODO: handle error writing");
                }
            }

            tracing::debug!("Creating file `{}` already complete", this.path.display());
            return Ok(StatefulAction::completed(this));
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_file")]
impl Action for CreateOrMergeNixConf {
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
        } = self;

        if tracing::enabled!(tracing::Level::TRACE) {
            let span = tracing::Span::current();
            span.record("buf", &buf);
        }

        let mut options = OpenOptions::new();
        options.create_new(true).write(true).read(true);

        if let Some(mode) = mode {
            options.mode(*mode);
        }

        let mut file = options
            .open(&path)
            .await
            .map_err(|e| ActionError::Open(path.to_owned(), e))?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| ActionError::Write(path.to_owned(), e))?;

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
        let mut action =
            CreateOrMergeNixConf::plan(test_file.clone(), None, None, None, "Test".into()).await?;

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
        let mut action =
            CreateOrMergeNixConf::plan(test_file.clone(), None, None, None, "Test".into()).await?;

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

        let test_content = "Some content";
        write(test_file.as_path(), test_content).await?;

        let mut action =
            CreateOrMergeNixConf::plan(test_file.clone(), None, None, None, test_content.into())
                .await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_different_files_and_errors() -> eyre::Result<()> {
        let temp_dir = tempdir::TempDir::new("nix_installer_tests_create_file")?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_different_files_and_errors");

        write(test_file.as_path(), "Some content").await?;

        match CreateOrMergeNixConf::plan(
            test_file.clone(),
            None,
            None,
            None,
            "Some different content".into(),
        )
        .await
        {
            Err(ActionError::Exists(path)) => assert_eq!(path, test_file.as_path()),
            _ => return Err(eyre!("Should have returned an ActionError::Exists error")),
        }

        assert!(test_file.exists(), "File should have not been deleted");

        Ok(())
    }
}
