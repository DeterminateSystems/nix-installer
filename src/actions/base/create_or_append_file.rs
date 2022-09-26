use nix::unistd::{chown, Group, User};
use serde::Serialize;
use std::{
    io::SeekFrom,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};

use crate::actions::{Action, ActionState};

use crate::actions::{ActionDescription, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateOrAppendFile {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
    buf: String,
    action_state: ActionState,
}

impl CreateOrAppendFile {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: String,
        group: String,
        mode: u32,
        buf: String,
    ) -> Result<Self, CreateOrAppendFileError> {
        let path = path.as_ref().to_path_buf();

        Ok(Self {
            path,
            user,
            group,
            mode,
            buf,
            action_state: ActionState::Planned,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for CreateOrAppendFile {
    type Error = CreateOrAppendFileError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
            action_state: _,
        } = &self;
        vec![ActionDescription::new(
            format!("Create or append file `{}`", path.display()),
            vec![format!(
                "Create or append `{}` owned by `{user}:{group}` with mode `{mode:#o}` with `{buf}`", path.display()
            )],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
            action_state,
        } = self;

        tracing::trace!(path = %path.display(), "Creating or appending");
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| Self::Error::OpenFile(path.to_owned(), e))?;

        file.seek(SeekFrom::End(0))
            .await
            .map_err(|e| Self::Error::SeekFile(path.to_owned(), e))?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| Self::Error::WriteFile(path.to_owned(), e))?;

        let gid = Group::from_name(group.as_str())
            .map_err(|e| Self::Error::GroupId(group.clone(), e))?
            .ok_or(Self::Error::NoGroup(group.clone()))?
            .gid;
        let uid = User::from_name(user.as_str())
            .map_err(|e| Self::Error::UserId(user.clone(), e))?
            .ok_or(Self::Error::NoUser(user.clone()))?
            .uid;

        tracing::trace!(path = %path.display(), "Changing permissions on file");
        tokio::fs::set_permissions(&path, PermissionsExt::from_mode(*mode))
            .await
            .map_err(|e| Self::Error::SetPermissions(*mode, path.to_owned(), e))?;

        tracing::trace!(path = %path.display(), "Chowning");
        chown(path, Some(uid), Some(gid)).map_err(|e| Self::Error::Chown(path.clone(), e))?;

        *action_state = ActionState::Completed;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf,
            action_state,
        } = self;
        tracing::trace!(path = %path.display(), "Deleting or trimming content from file");

        let mut file = OpenOptions::new()
            .create(false)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| Self::Error::ReadFile(path.to_owned(), e))?;

        let mut file_contents = String::default();
        file.read_to_string(&mut file_contents)
            .await
            .map_err(|e| Self::Error::SeekFile(path.to_owned(), e))?;

        if let Some(start) = file_contents.rfind(buf.as_str()) {
            let end = start + buf.len();
            file_contents.replace_range(start..end, "")
        }

        if buf.is_empty() {
            remove_file(&path)
                .await
                .map_err(|e| Self::Error::RemoveFile(path.to_owned(), e))?;
        } else {
            file.seek(SeekFrom::Start(0))
                .await
                .map_err(|e| Self::Error::SeekFile(path.to_owned(), e))?;
            file.write_all(file_contents.as_bytes())
                .await
                .map_err(|e| Self::Error::WriteFile(path.to_owned(), e))?;
        }

        *action_state = ActionState::Reverted;
        Ok(())
    }
}

impl From<CreateOrAppendFile> for Action {
    fn from(v: CreateOrAppendFile) -> Self {
        Action::CreateOrAppendFile(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateOrAppendFileError {
    #[error("Remove file `{0}`")]
    RemoveFile(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Remove file `{0}`")]
    ReadFile(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Open file `{0}`")]
    OpenFile(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Write file `{0}`")]
    WriteFile(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Seek file `{0}`")]
    SeekFile(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Getting uid for user `{0}`")]
    UserId(
        String,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        nix::errno::Errno,
    ),
    #[error("Getting user `{0}`")]
    NoUser(String),
    #[error("Getting gid for group `{0}`")]
    GroupId(
        String,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        nix::errno::Errno,
    ),
    #[error("Getting group `{0}`")]
    NoGroup(String),
    #[error("Set mode `{0}` on `{1}`")]
    SetPermissions(
        u32,
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Chowning directory `{0}`")]
    Chown(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        nix::errno::Errno,
    ),
}
