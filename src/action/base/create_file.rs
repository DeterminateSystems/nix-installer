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

use crate::action::{Action, ActionDescription, ActionError, ActionTag, StatefulAction};

/** Create a file at the given location with the provided `buf`,
optionally with an owning user, group, and mode.

If `force` is set, the file will always be overwritten (and deleted)
regardless of its presence prior to install.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
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
                .map_err(|e| ActionError::Open(this.path.clone(), e))?;

            let metadata = file
                .metadata()
                .await
                .map_err(|e| ActionError::GettingMetadata(this.path.clone(), e))?;

            if !metadata.is_file() {
                return Err(ActionError::PathWasNotFile(this.path));
            }

            if let Some(mode) = mode {
                // Does the file have the right permissions?
                let discovered_mode = metadata.permissions().mode();
                // We only care about user-group-other permissions
                let discovered_mode = discovered_mode & 0o777;

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

            // Does it have the right content?
            let mut discovered_buf = String::new();
            file.read_to_string(&mut discovered_buf)
                .await
                .map_err(|e| ActionError::Read(this.path.clone(), e))?;

            if discovered_buf != this.buf {
                return Err(ActionError::DifferentContent(this.path.clone()));
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
            force: _,
        } = &self;

        vec![ActionDescription::new(
            format!("Delete file `{}`", path.display()),
            vec![format!("Delete file `{}`", path.display())],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), Vec<ActionError>> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf: _,
            force: _,
        } = self;

        remove_file(&path)
            .await
            .map_err(|e| vec![ActionError::Remove(path.to_owned(), e)])?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use color_eyre::{eyre::eyre, Section};
    use tokio::fs::write;

    #[tokio::test]
    async fn creates_and_deletes_file() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("creates_and_deletes_file");
        let mut action =
            CreateFile::plan(test_file.clone(), None, None, None, "Test".into(), false).await?;

        action.try_execute().await?;

        if let Err(errs) = action.try_revert().await {
            let mut report = eyre!("Errors");
            for err in errs {
                report = report.error(err);
            }
        }

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

        if let Err(errs) = action.try_revert().await {
            let mut report = eyre!("Errors");
            for err in errs {
                report = report.error(err);
            }
        }

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

        if let Err(errs) = action.try_revert().await {
            let mut report = eyre!("Errors");
            for err in errs {
                report = report.error(err);
            }
        }

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
            Err(ActionError::DifferentContent(path)) => assert_eq!(path, test_file.as_path()),
            _ => return Err(eyre!("Should have returned an ActionError::Exists error")),
        }

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
            Err(ActionError::PathModeMismatch(path, got, expected)) => {
                assert_eq!(path, test_file.as_path());
                assert_eq!(expected, expected_mode);
                assert_eq!(got, initial_mode);
            },
            _ => {
                return Err(eyre!(
                    "Should have returned an ActionError::PathModeMismatch error"
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

        if let Err(errs) = action.try_revert().await {
            let mut report = eyre!("Errors");
            for err in errs {
                report = report.error(err);
            }
        }

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
            Err(ActionError::PathWasNotFile(path)) => assert_eq!(path, temp_dir.path()),
            _ => {
                return Err(eyre!(
                    "Should have returned an ActionError::PathWasNotFile error"
                ))
            },
        }

        Ok(())
    }
}
