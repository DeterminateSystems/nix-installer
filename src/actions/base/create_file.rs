use nix::unistd::{chown, Group, User};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::AsyncWriteExt,
};

use crate::actions::{Action, ActionState};

use crate::actions::{ActionDescription, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateFile {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
    buf: String,
    force: bool,
    action_state: ActionState,
}

impl CreateFile {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: String,
        group: String,
        mode: u32,
        buf: String,
        force: bool,
    ) -> Result<Self, CreateFileError> {
        let path = path.as_ref().to_path_buf();

        if path.exists() && !force {
            return Err(CreateFileError::Exists(path.to_path_buf()));
        }

        Ok(Self {
            path,
            user,
            group,
            mode,
            buf,
            force,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for CreateFile {
    type Error = CreateFileError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
            force: _,
            action_state: _,
        } = &self;
        vec![ActionDescription::new(
            format!("Create or overwrite file `{}`", path.display()),
            vec![format!(
                "Create or overwrite `{}` owned by `{user}:{group}` with mode `{mode:#o}` with `{buf}`", path.display()
            )],
        )]
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
        user = self.user,
        group = self.group,
        mode = format!("{:#o}", self.mode),
    ))]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
            force: _,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating file");
            return Ok(());
        }
        tracing::debug!("Creating file");

        let mut file = OpenOptions::new()
            .create_new(true)
            .mode(*mode)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| Self::Error::OpenFile(path.to_owned(), e))?;

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

        chown(path, Some(uid), Some(gid)).map_err(|e| Self::Error::Chown(path.clone(), e))?;

        tracing::trace!("Created file");
        *action_state = ActionState::Completed;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
        user = self.user,
        group = self.group,
        mode = format!("{:#o}", self.mode),
    ))]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf: _,
            force: _,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Deleting file");
            return Ok(());
        }
        tracing::debug!("Deleting file");

        remove_file(&path)
            .await
            .map_err(|e| Self::Error::RemoveFile(path.to_owned(), e))?;

        tracing::trace!("Deleted file");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }
}

impl From<CreateFile> for Action {
    fn from(v: CreateFile) -> Self {
        Action::CreateFile(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateFileError {
    #[error("File exists `{0}`")]
    Exists(std::path::PathBuf),
    #[error("Remove file `{0}`")]
    RemoveFile(
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
    #[error("Chowning directory `{0}`")]
    Chown(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        nix::errno::Errno,
    ),
}
