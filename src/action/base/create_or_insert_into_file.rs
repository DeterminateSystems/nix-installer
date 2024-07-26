use nix::unistd::{chown, Group, User};

use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};
use rand::Rng;
use std::{
    io::SeekFrom,
    os::{unix::fs::MetadataExt, unix::prelude::PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::{
    fs::{remove_file, File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};
use tracing::{span, Span};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, PartialEq, Eq)]
pub enum Position {
    Beginning,
    End,
}

/** Create a file at the given location with the provided `buf` as
contents, optionally with an owning user, group, and mode.

If the file exists, the provided `buf` will be inserted at its
beginning or end, depending on the position field.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_or_insert_into_file")]
pub struct CreateOrInsertIntoFile {
    path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    buf: String,
    position: Position,
}

impl CreateOrInsertIntoFile {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        buf: String,
        position: Position,
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
            position,
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
                    tracing::debug!(
                        "`{}` has mode `{}`, a mode of `{}` was expected",
                        this.path.display(),
                        discovered_mode,
                        mode,
                    );
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

            if discovered_buf.contains(&this.buf) {
                tracing::debug!("Inserting into `{}` already complete", this.path.display(),);
                return Ok(StatefulAction::completed(this));
            }

            // If not, we can't skip this, so we still do it
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_or_insert_into_file")]
impl Action for CreateOrInsertIntoFile {
    fn action_tag() -> ActionTag {
        ActionTag("create_or_insert_into_file")
    }
    fn tracing_synopsis(&self) -> String {
        format!("Create or insert file `{}`", self.path.display())
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_or_insert_file",
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
            position,
        } = self;

        let mut orig_file = match OpenOptions::new().read(true).open(&path).await {
            Ok(f) => Some(f),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(Self::error(ActionErrorKind::Open(path.to_owned(), e))),
        };

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
                ActionErrorKind::Open(temp_file_path.clone(), e)
            }).map_err(Self::error)?;

        if *position == Position::End {
            if let Some(ref mut orig_file) = orig_file {
                tokio::io::copy(orig_file, &mut temp_file)
                    .await
                    .map_err(|e| {
                        ActionErrorKind::Copy(path.to_owned(), temp_file_path.to_owned(), e)
                    })
                    .map_err(Self::error)?;
            }
        }

        temp_file
            .write_all(buf.as_bytes())
            .await
            .map_err(|e| ActionErrorKind::Write(temp_file_path.clone(), e))
            .map_err(Self::error)?;

        if *position == Position::Beginning {
            if let Some(ref mut orig_file) = orig_file {
                tokio::io::copy(orig_file, &mut temp_file)
                    .await
                    .map_err(|e| {
                        ActionErrorKind::Copy(path.to_owned(), temp_file_path.to_owned(), e)
                    })
                    .map_err(Self::error)?;
            }
        }

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

        // Change ownership _before_ applying mode, to ensure that if
        // a file needs to be setuid it will never be setuid for the
        // wrong user
        chown(&temp_file_path, uid, gid)
            .map_err(|e| ActionErrorKind::Chown(path.clone(), e))
            .map_err(Self::error)?;

        if let Some(mode) = mode {
            tokio::fs::set_permissions(&temp_file_path, PermissionsExt::from_mode(*mode))
                .await
                .map_err(|e| ActionErrorKind::SetPermissions(*mode, path.to_owned(), e))
                .map_err(Self::error)?;
        } else if let Some(original_file) = orig_file {
            let original_file_mode = original_file
                .metadata()
                .await
                .map_err(|e| ActionErrorKind::GettingMetadata(path.to_path_buf(), e))
                .map_err(Self::error)?
                .permissions()
                .mode();
            tokio::fs::set_permissions(
                &temp_file_path,
                PermissionsExt::from_mode(original_file_mode),
            )
            .await
            .map_err(|e| ActionErrorKind::SetPermissions(original_file_mode, path.to_owned(), e))
            .map_err(Self::error)?;
        }

        tokio::fs::rename(&temp_file_path, &path)
            .await
            .map_err(|e| ActionErrorKind::Rename(path.to_owned(), temp_file_path.to_owned(), e))
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf,
            position: _,
        } = &self;
        vec![ActionDescription::new(
            format!("Delete Nix related fragment from file `{}`", path.display()),
            vec![format!(
                "Delete Nix related fragment from file `{}`. Fragment: `{buf}`",
                path.display()
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf,
            position: _,
        } = self;
        // The user already deleted it
        if !path.exists() {
            return Ok(());
        }

        let mut file = OpenOptions::new()
            .create(false)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| ActionErrorKind::Open(path.to_owned(), e))
            .map_err(Self::error)?;

        let mut file_contents = String::default();
        file.read_to_string(&mut file_contents)
            .await
            .map_err(|e| ActionErrorKind::Read(path.to_owned(), e))
            .map_err(Self::error)?;

        if let Some(start) = file_contents.rfind(buf.as_str()) {
            let end = start + buf.len();
            file_contents.replace_range(start..end, "")
        }

        if file_contents.is_empty() {
            remove_file(&path)
                .await
                .map_err(|e| ActionErrorKind::Remove(path.to_owned(), e))
                .map_err(Self::error)?;
        } else {
            file.seek(SeekFrom::Start(0))
                .await
                .map_err(|e| ActionErrorKind::Seek(path.to_owned(), e))
                .map_err(Self::error)?;
            file.set_len(0)
                .await
                .map_err(|e| ActionErrorKind::Truncate(path.to_owned(), e))
                .map_err(Self::error)?;
            file.write_all(file_contents.as_bytes())
                .await
                .map_err(|e| ActionErrorKind::Write(path.to_owned(), e))
                .map_err(Self::error)?;
            file.flush()
                .await
                .map_err(|e| ActionErrorKind::Flush(path.to_owned(), e))
                .map_err(Self::error)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use color_eyre::eyre::eyre;
    use tokio::fs::{read_to_string, write};

    #[tokio::test]
    async fn creates_and_deletes_file() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("creates_and_deletes_file");
        let mut action = CreateOrInsertIntoFile::plan(
            test_file.clone(),
            None,
            None,
            None,
            "Test".into(),
            Position::Beginning,
        )
        .await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn edits_and_reverts_file() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("edits_and_reverts_file");

        let test_content = "Some other content";
        tokio::fs::write(&test_file, test_content)
            .await
            .expect("Could not write to test temp file");

        let mut action = CreateOrInsertIntoFile::plan(
            test_file.clone(),
            None,
            None,
            None,
            "Test".into(),
            Position::Beginning,
        )
        .await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(test_file.exists(), "File should have not been deleted");

        let read_content = tokio::fs::read_to_string(test_file)
            .await
            .expect("Could not read test temp file");

        assert_eq!(test_content, read_content);

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_containing_exact_contents_and_reverts_it() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_containing_exact_contents_and_reverts_it");

        let expected_content = "Some expected content";
        write(test_file.as_path(), expected_content).await?;

        let added_content = "\nSome more expected content";
        write(test_file.as_path(), added_content).await?;

        // We test all `Position` options
        let positions = [Position::Beginning, Position::End];
        for position in positions {
            let mut action = CreateOrInsertIntoFile::plan(
                test_file.clone(),
                None,
                None,
                None,
                expected_content.into(),
                position,
            )
            .await?;

            action.try_execute().await?;

            action.try_revert().await?;

            assert!(test_file.exists(), "File should have not been deleted");
            let after_revert_content = read_to_string(&test_file).await?;
            assert_eq!(after_revert_content, added_content);
        }

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_wrong_mode_and_does_not_error() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir
            .path()
            .join("recognizes_wrong_mode_and_does_not_error");
        let initial_mode = 0o777;
        let expected_mode = 0o666;

        write(test_file.as_path(), "Some content").await?;
        tokio::fs::set_permissions(test_file.as_path(), PermissionsExt::from_mode(initial_mode))
            .await?;

        let mut action = CreateOrInsertIntoFile::plan(
            test_file.clone(),
            None,
            None,
            Some(expected_mode),
            "Some different content".into(),
            Position::End,
        )
        .await?;

        action.try_execute().await?;

        action.try_revert().await?;

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

        let mut action = CreateOrInsertIntoFile::plan(
            test_file.clone(),
            None,
            None,
            Some(initial_mode),
            "Some content".into(),
            Position::End,
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

        match CreateOrInsertIntoFile::plan(
            temp_dir.path(),
            None,
            None,
            None,
            "Some different content".into(),
            Position::End,
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
