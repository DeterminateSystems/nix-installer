use nix::unistd::{chown, Group, User};

use std::{
    io::SeekFrom,
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};

use crate::{
    action::{Action, ActionDescription, StatefulAction},
    BoxableError,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateOrAppendFile {
    path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    buf: String,
}

impl CreateOrAppendFile {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        buf: String,
    ) -> Result<StatefulAction<Self>, CreateOrAppendFileError> {
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

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
        user = self.user,
        group = self.group,
        mode = self.mode.map(|v| tracing::field::display(format!("{:#o}", v))),
    ))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
        } = self;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| CreateOrAppendFileError::OpenFile(path.to_owned(), e).boxed())?;

        file.seek(SeekFrom::End(0))
            .await
            .map_err(|e| CreateOrAppendFileError::SeekFile(path.to_owned(), e).boxed())?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| CreateOrAppendFileError::WriteFile(path.to_owned(), e).boxed())?;

        let gid = if let Some(group) = group {
            Some(
                Group::from_name(group.as_str())
                    .map_err(|e| CreateOrAppendFileError::GroupId(group.clone(), e).boxed())?
                    .ok_or(CreateOrAppendFileError::NoGroup(group.clone()).boxed())?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| CreateOrAppendFileError::UserId(user.clone(), e).boxed())?
                    .ok_or(CreateOrAppendFileError::NoUser(user.clone()).boxed())?
                    .uid,
            )
        } else {
            None
        };

        if let Some(mode) = mode {
            tokio::fs::set_permissions(&path, PermissionsExt::from_mode(*mode))
                .await
                .map_err(|e| {
                    CreateOrAppendFileError::SetPermissions(*mode, path.to_owned(), e).boxed()
                })?;
        }

        chown(path, uid, gid)
            .map_err(|e| CreateOrAppendFileError::Chown(path.clone(), e).boxed())?;

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

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
        user = self.user,
        group = self.group,
        mode = self.mode.map(|v| tracing::field::display(format!("{:#o}", v))),
    ))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
            .map_err(|e| CreateOrAppendFileError::ReadFile(path.to_owned(), e).boxed())?;

        let mut file_contents = String::default();
        file.read_to_string(&mut file_contents)
            .await
            .map_err(|e| CreateOrAppendFileError::SeekFile(path.to_owned(), e).boxed())?;

        if let Some(start) = file_contents.rfind(buf.as_str()) {
            let end = start + buf.len();
            file_contents.replace_range(start..end, "")
        }

        if buf.is_empty() {
            remove_file(&path)
                .await
                .map_err(|e| CreateOrAppendFileError::RemoveFile(path.to_owned(), e).boxed())?;
        } else {
            file.seek(SeekFrom::Start(0))
                .await
                .map_err(|e| CreateOrAppendFileError::SeekFile(path.to_owned(), e).boxed())?;
            file.write_all(file_contents.as_bytes())
                .await
                .map_err(|e| CreateOrAppendFileError::WriteFile(path.to_owned(), e).boxed())?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateOrAppendFileError {
    #[error("Remove file `{0}`")]
    RemoveFile(std::path::PathBuf, #[source] std::io::Error),
    #[error("Remove file `{0}`")]
    ReadFile(std::path::PathBuf, #[source] std::io::Error),
    #[error("Open file `{0}`")]
    OpenFile(std::path::PathBuf, #[source] std::io::Error),
    #[error("Write file `{0}`")]
    WriteFile(std::path::PathBuf, #[source] std::io::Error),
    #[error("Seek file `{0}`")]
    SeekFile(std::path::PathBuf, #[source] std::io::Error),
    #[error("Getting uid for user `{0}`")]
    UserId(String, #[source] nix::errno::Errno),
    #[error("Getting user `{0}`")]
    NoUser(String),
    #[error("Getting gid for group `{0}`")]
    GroupId(String, #[source] nix::errno::Errno),
    #[error("Getting group `{0}`")]
    NoGroup(String),
    #[error("Set mode `{0}` on `{1}`")]
    SetPermissions(u32, std::path::PathBuf, #[source] std::io::Error),
    #[error("Chowning directory `{0}`")]
    Chown(std::path::PathBuf, #[source] nix::errno::Errno),
}
