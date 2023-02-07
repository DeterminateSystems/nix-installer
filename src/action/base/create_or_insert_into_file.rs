use nix::unistd::{chown, Group, User};

use crate::action::{Action, ActionDescription, ActionError, StatefulAction};
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
                .map_err(|e| ActionError::Open(this.path.clone(), e))?;

            let metadata = file
                .metadata()
                .await
                .map_err(|e| ActionError::GettingMetadata(this.path.clone(), e))?;
            if let Some(mode) = mode {
                // Does the file have the right permissions?
                let discovered_mode = metadata.permissions().mode();
                if discovered_mode != mode {
                    return Err(ActionError::FileModeMismatch(
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
                if found_uid == expected_uid.as_raw() {
                    return Err(ActionError::FileUserMismatch(
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
                if found_gid == expected_gid.as_raw() {
                    return Err(ActionError::FileGroupMismatch(
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

            if discovered_buf.contains(&this.buf) {
                return Ok(StatefulAction::skipped(this));
            }

            // If not, we can't skip this, so we still do it
        }

        Ok(this.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_or_insert_into_file")]
impl Action for CreateOrInsertIntoFile {
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
            Err(e) => return Err(ActionError::Open(path.to_owned(), e)),
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

        if *position == Position::End {
            if let Some(ref mut orig_file) = orig_file {
                tokio::io::copy(orig_file, &mut temp_file)
                    .await
                    .map_err(|e| {
                        ActionError::Copy(path.to_owned(), temp_file_path.to_owned(), e)
                    })?;
            }
        }

        temp_file
            .write_all(buf.as_bytes())
            .await
            .map_err(|e| ActionError::Write(temp_file_path.clone(), e))?;

        if *position == Position::Beginning {
            if let Some(ref mut orig_file) = orig_file {
                tokio::io::copy(orig_file, &mut temp_file)
                    .await
                    .map_err(|e| {
                        ActionError::Copy(path.to_owned(), temp_file_path.to_owned(), e)
                    })?;
            }
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

        // Change ownership _before_ applying mode, to ensure that if
        // a file needs to be setuid it will never be setuid for the
        // wrong user
        chown(&temp_file_path, uid, gid).map_err(|e| ActionError::Chown(path.clone(), e))?;

        if let Some(mode) = mode {
            tokio::fs::set_permissions(&temp_file_path, PermissionsExt::from_mode(*mode))
                .await
                .map_err(|e| ActionError::SetPermissions(*mode, path.to_owned(), e))?;
        } else if let Some(original_file) = orig_file {
            let original_file_mode = original_file
                .metadata()
                .await
                .map_err(|e| ActionError::GettingMetadata(path.to_path_buf(), e))?
                .permissions()
                .mode();
            tokio::fs::set_permissions(
                &temp_file_path,
                PermissionsExt::from_mode(original_file_mode),
            )
            .await
            .map_err(|e| ActionError::SetPermissions(original_file_mode, path.to_owned(), e))?;
        }

        tokio::fs::rename(&temp_file_path, &path)
            .await
            .map_err(|e| ActionError::Rename(path.to_owned(), temp_file_path.to_owned(), e))?;

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
        let mut file = OpenOptions::new()
            .create(false)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| ActionError::Open(path.to_owned(), e))?;

        let mut file_contents = String::default();
        file.read_to_string(&mut file_contents)
            .await
            .map_err(|e| ActionError::Read(path.to_owned(), e))?;

        if let Some(start) = file_contents.rfind(buf.as_str()) {
            let end = start + buf.len();
            file_contents.replace_range(start..end, "")
        }

        if file_contents.is_empty() {
            remove_file(&path)
                .await
                .map_err(|e| ActionError::Remove(path.to_owned(), e))?;
        } else {
            file.seek(SeekFrom::Start(0))
                .await
                .map_err(|e| ActionError::Seek(path.to_owned(), e))?;
            file.set_len(0)
                .await
                .map_err(|e| ActionError::Truncate(path.to_owned(), e))?;
            file.write_all(file_contents.as_bytes())
                .await
                .map_err(|e| ActionError::Write(path.to_owned(), e))?;
            file.flush()
                .await
                .map_err(|e| ActionError::Flush(path.to_owned(), e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn creates_and_deletes_file() -> eyre::Result<()> {
        let temp_dir = tempdir::TempDir::new("nix_installer_create_or_insert_into_file")?;
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
        let temp_dir = tempdir::TempDir::new("nix_installer_create_or_insert_into_file")?;
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
}
