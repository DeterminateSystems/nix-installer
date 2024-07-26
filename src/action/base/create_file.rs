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

use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

/** Create a file at the given location with the provided `buf`,
optionally with an owning user, group, and mode.

If `force` is set, the file will always be overwritten (and deleted)
regardless of its presence prior to install.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_file")]
pub struct CreateFile {
    pub(crate) path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    buf: String,
    force: bool,
}

impl CreateFile {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        buf: String,
        force: bool,
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
            force,
        };

        if this.path.exists() {
            // If the path exists, perhaps we can just skip this
            let mut file = File::open(&this.path)
                .await
                .map_err(|e| ActionErrorKind::Open(this.path.clone(), e))
                .map_err(Self::error)?;

            let metadata = file
                .metadata()
                .await
                .map_err(|e| ActionErrorKind::GettingMetadata(this.path.clone(), e))
                .map_err(Self::error)?;

            if !metadata.is_file() {
                return Err(Self::error(ActionErrorKind::PathWasNotFile(this.path)));
            }

            if let Some(mode) = mode {
                // Does the file have the right permissions?
                let discovered_mode = metadata.permissions().mode();
                // We only care about user-group-other permissions
                let discovered_mode = discovered_mode & 0o777;

                if discovered_mode != mode {
                    return Err(Self::error(ActionErrorKind::PathModeMismatch(
                        this.path.clone(),
                        discovered_mode,
                        mode,
                    )));
                }
            }

            // Does it have the right user/group?
            if let Some(user) = &this.user {
                // If the file exists, the user must also exist to be correct.
                let expected_uid = User::from_name(user.as_str())
                    .map_err(|e| ActionErrorKind::GettingUserId(user.clone(), e))
                    .map_err(Self::error)?
                    .ok_or_else(|| ActionErrorKind::NoUser(user.clone()))
                    .map_err(Self::error)?
                    .uid;
                let found_uid = metadata.uid();
                if found_uid != expected_uid.as_raw() {
                    return Err(Self::error(ActionErrorKind::PathUserMismatch(
                        this.path.clone(),
                        found_uid,
                        expected_uid.as_raw(),
                    )));
                }
            }
            if let Some(group) = &this.group {
                // If the file exists, the group must also exist to be correct.
                let expected_gid = Group::from_name(group.as_str())
                    .map_err(|e| ActionErrorKind::GettingGroupId(group.clone(), e))
                    .map_err(Self::error)?
                    .ok_or_else(|| ActionErrorKind::NoUser(group.clone()))
                    .map_err(Self::error)?
                    .gid;
                let found_gid = metadata.gid();
                if found_gid != expected_gid.as_raw() {
                    return Err(Self::error(ActionErrorKind::PathGroupMismatch(
                        this.path.clone(),
                        found_gid,
                        expected_gid.as_raw(),
                    )));
                }
            }

            // Does it have the right content?
            let mut discovered_buf = String::new();
            file.read_to_string(&mut discovered_buf)
                .await
                .map_err(|e| ActionErrorKind::Read(this.path.clone(), e))
                .map_err(Self::error)?;

            if discovered_buf != this.buf {
                return Err(Self::error(ActionErrorKind::DifferentContent(
                    this.path.clone(),
                )));
            }

            tracing::debug!("Creating file `{}` already complete", this.path.display());
            return Ok(StatefulAction::completed(this));
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_file")]
impl Action for CreateFile {
    fn action_tag() -> ActionTag {
        ActionTag("create_file")
    }
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
            force: _,
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
            .map_err(|e| ActionErrorKind::Open(path.to_owned(), e))
            .map_err(Self::error)?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| ActionErrorKind::Write(path.to_owned(), e))
            .map_err(Self::error)?;

        let gid = if let Some(group) = group {
            Some(
                Group::from_name(group.as_str())
                    .map_err(|e| ActionErrorKind::GettingGroupId(group.clone(), e))
                    .map_err(Self::error)?
                    .ok_or(ActionErrorKind::NoGroup(group.clone()))
                    .map_err(Self::error)?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| ActionErrorKind::GettingUserId(user.clone(), e))
                    .map_err(Self::error)?
                    .ok_or(ActionErrorKind::NoUser(user.clone()))
                    .map_err(Self::error)?
                    .uid,
            )
        } else {
            None
        };
        chown(path, uid, gid)
            .map_err(|e| ActionErrorKind::Chown(path.clone(), e))
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf: _,
            force: _,
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
            force: _,
        } = self;
        // The user already deleted it
        if !path.exists() {
            return Ok(());
        }

        remove_file(&path)
            .await
            .map_err(|e| ActionErrorKind::Remove(path.to_owned(), e))
            .map_err(Self::error)?;

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
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("creates_and_deletes_file");
        let mut action =
            CreateFile::plan(test_file.clone(), None, None, None, "Test".into(), false).await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn creates_and_deletes_file_even_if_edited() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir
            .path()
            .join("creates_and_deletes_file_even_if_edited");
        let mut action =
            CreateFile::plan(test_file.clone(), None, None, None, "Test".into(), false).await?;

        action.try_execute().await?;

        write(test_file.as_path(), "More content").await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_exact_files_and_reverts_them() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_exact_files_and_reverts_them");

        let test_content = "Some content";
        write(test_file.as_path(), test_content).await?;

        let mut action = CreateFile::plan(
            test_file.clone(),
            None,
            None,
            None,
            test_content.into(),
            false,
        )
        .await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_different_files_and_errors() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_different_files_and_errors");

        write(test_file.as_path(), "Some content").await?;

        match CreateFile::plan(
            test_file.clone(),
            None,
            None,
            None,
            "Some different content".into(),
            false,
        )
        .await
        {
            Err(error) => match error.kind() {
                ActionErrorKind::DifferentContent(path) => assert_eq!(path, test_file.as_path()),
                _ => {
                    return Err(eyre!(
                        "Should have returned an ActionErrorKind::Exists error"
                    ))
                },
            },
            _ => {
                return Err(eyre!(
                    "Should have returned an ActionErrorKind::Exists error"
                ))
            },
        };

        assert!(test_file.exists(), "File should have not been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_wrong_mode_and_errors() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("recognizes_wrong_mode_and_errors");
        let initial_mode = 0o777;
        let expected_mode = 0o000;

        write(test_file.as_path(), "Some content").await?;
        tokio::fs::set_permissions(test_file.as_path(), PermissionsExt::from_mode(initial_mode))
            .await?;

        match CreateFile::plan(
            test_file.clone(),
            None,
            None,
            Some(expected_mode),
            "Some different content".into(),
            false,
        )
        .await
        {
            Err(err) => match err.kind() {
                ActionErrorKind::PathModeMismatch(path, got, expected) => {
                    assert_eq!(path, test_file.as_path());
                    assert_eq!(*expected, expected_mode);
                    assert_eq!(*got, initial_mode);
                },
                _ => {
                    return Err(eyre!(
                        "Should have returned an ActionErrorKind::PathModeMismatch error"
                    ))
                },
            },
            _ => {
                return Err(eyre!(
                    "Should have returned an ActionErrorKind::PathModeMismatch error"
                ))
            },
        }

        assert!(test_file.exists(), "File should have not been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_correct_mode() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("recognizes_correct_mode");
        let initial_mode = 0o777;

        write(test_file.as_path(), "Some content").await?;
        tokio::fs::set_permissions(test_file.as_path(), PermissionsExt::from_mode(initial_mode))
            .await?;

        let mut action = CreateFile::plan(
            test_file.clone(),
            None,
            None,
            Some(initial_mode),
            "Some content".into(),
            false,
        )
        .await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn errors_on_dir() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;

        match CreateFile::plan(
            temp_dir.path(),
            None,
            None,
            None,
            "Some different content".into(),
            false,
        )
        .await
        {
            Err(err) => match err.kind() {
                ActionErrorKind::PathWasNotFile(path) => assert_eq!(path, temp_dir.path()),
                _ => {
                    return Err(eyre!(
                        "Should have returned an ActionErrorKind::PathWasNotFile error"
                    ))
                },
            },
            _ => {
                return Err(eyre!(
                    "Should have returned an ActionErrorKind::PathWasNotFile error"
                ))
            },
        }

        Ok(())
    }
}
