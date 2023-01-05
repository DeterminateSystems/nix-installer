use nix::unistd::{chown, Group, User};

use crate::action::{Action, ActionDescription, ActionError, StatefulAction};
use rand::Rng;
use std::{
    io::SeekFrom,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};
use tracing::{span, Span};

/** Create a file at the given location with the provided `buf`,
optionally with an owning user, group, and mode.

If the file exists, the provided `buf` will be inserted at its beginning.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateOrInsertFile {
    path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    buf: String,
}

impl CreateOrInsertFile {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        buf: String,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref().to_path_buf();

        Ok(Self {
            path,
            user: user.into(),
            group: group.into(),
            mode: mode.into(),
            buf,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_or_insert_file")]
impl Action for CreateOrInsertFile {
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

        temp_file
            .write_all(buf.as_bytes())
            .await
            .map_err(|e| ActionError::Write(temp_file_path.clone(), e))?;

        let gid = if let Some(group) = group {
            Some(
                Group::from_name(group.as_str())
                    .map_err(|e| ActionError::GroupId(group.clone(), e))?
                    .ok_or(ActionError::NoGroup(group.clone()))?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| ActionError::UserId(user.clone(), e))?
                    .ok_or(ActionError::NoUser(user.clone()))?
                    .uid,
            )
        } else {
            None
        };

        if let Some(ref mut orig_file) = orig_file {
            tokio::io::copy(orig_file, &mut temp_file)
                .await
                .map_err(|e| ActionError::Copy(path.to_owned(), temp_file_path.to_owned(), e))?;
        }

        // Change ownership _before_ applying mode, to ensure that if
        // a file needs to be setuid it will never be setuid for the
        // wrong user
        chown(&temp_file_path, uid, gid).map_err(|e| ActionError::Chown(path.clone(), e))?;

        if let Some(mode) = mode {
            tokio::fs::set_permissions(&temp_file_path, PermissionsExt::from_mode(*mode))
                .await
                .map_err(|e| ActionError::SetPermissions(*mode, path.to_owned(), e))?;
        } else if orig_file.is_some() {
            tokio::fs::set_permissions(&temp_file_path, PermissionsExt::from_mode(0o644))
                .await
                .map_err(|e| ActionError::SetPermissions(0o644, path.to_owned(), e))?;
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
        } = self;
        let mut file = OpenOptions::new()
            .create(false)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| ActionError::Read(path.to_owned(), e))?;

        let mut file_contents = String::default();
        file.read_to_string(&mut file_contents)
            .await
            .map_err(|e| ActionError::Seek(path.to_owned(), e))?;

        if let Some(start) = file_contents.rfind(buf.as_str()) {
            let end = start + buf.len();
            file_contents.replace_range(start..end, "")
        }

        if buf.is_empty() {
            remove_file(&path)
                .await
                .map_err(|e| ActionError::Remove(path.to_owned(), e))?;
        } else {
            file.seek(SeekFrom::Start(0))
                .await
                .map_err(|e| ActionError::Seek(path.to_owned(), e))?;
            file.write_all(file_contents.as_bytes())
                .await
                .map_err(|e| ActionError::Write(path.to_owned(), e))?;
        }
        Ok(())
    }
}
