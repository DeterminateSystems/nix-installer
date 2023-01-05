use nix::unistd::{chown, Group, User};

use crate::action::{Action, ActionDescription, ActionError, StatefulAction};
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

If the file exists, the provided `buf` will be appended.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateOrAppendFile {
    path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    buf: String,
}

impl CreateOrAppendFile {
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
#[typetag::serde(name = "create_or_append_file")]
impl Action for CreateOrAppendFile {
    fn tracing_synopsis(&self) -> String {
        format!("Create or append file `{}`", self.path.display())
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_or_append_file",
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

        let existed = match tokio::fs::metadata(&path).await {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(ActionError::GettingMetadata(path.to_owned(), e)),
        };

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            // If the file is created, ensure that it has harmless
            // permissions regardless of whether the mode will be
            // changed later (if we ever create setuid executables,
            // they should only become setuid once they are owned by
            // the appropriate user)
            .mode(0o644 & mode.unwrap_or(0o644))
            .open(&path)
            .await
            .map_err(|e| ActionError::Open(path.to_owned(), e))?;

        file.seek(SeekFrom::End(0))
            .await
            .map_err(|e| ActionError::Seek(path.to_owned(), e))?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| ActionError::Write(path.to_owned(), e))?;

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

        // Change ownership _before_ applying mode, to ensure that if
        // a file needs to be setuid it will never be setuid for the
        // wrong user
        chown(path, uid, gid).map_err(|e| ActionError::Chown(path.clone(), e))?;

        if let Some(mode) = mode {
            tokio::fs::set_permissions(&path, PermissionsExt::from_mode(*mode))
                .await
                .map_err(|e| ActionError::SetPermissions(*mode, path.to_owned(), e))?;
        } else if !existed {
            tokio::fs::set_permissions(&path, PermissionsExt::from_mode(0o644))
                .await
                .map_err(|e| ActionError::SetPermissions(0o644, path.to_owned(), e))?;
        }

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
