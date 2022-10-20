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
    pub(crate) path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    buf: String,
    force: bool,
    action_state: ActionState,
}

impl CreateFile {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        buf: String,
        force: bool,
    ) -> Result<Self, CreateFileError> {
        let path = path.as_ref().to_path_buf();

        if path.exists() && !force {
            return Err(CreateFileError::Exists(path.to_path_buf()));
        }

        Ok(Self {
            path,
            user: user.into(),
            group: group.into(),
            mode: mode.into(),
            buf,
            force,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for CreateFile {
    type Error = CreateFileError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
            force: _,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Create or overwrite file `{}`", path.display()),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
        user = self.user,
        group = self.group,
        mode = self.mode.map(|v| tracing::field::display(format!("{:#o}", v))),
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

        let mut options = OpenOptions::new();
        options.create_new(true).write(true).read(true);

        if let Some(mode) = mode {
            options.mode(*mode);
        }

        let mut file = options
            .open(&path)
            .await
            .map_err(|e| Self::Error::OpenFile(path.to_owned(), e))?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| Self::Error::WriteFile(path.to_owned(), e))?;

        let gid = if let Some(group) = group {
            Some(
                Group::from_name(group.as_str())
                    .map_err(|e| Self::Error::GroupId(group.clone(), e))?
                    .ok_or(Self::Error::NoGroup(group.clone()))?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| Self::Error::UserId(user.clone(), e))?
                    .ok_or(Self::Error::NoUser(user.clone()))?
                    .uid,
            )
        } else {
            None
        };
        chown(path, uid, gid).map_err(|e| Self::Error::Chown(path.clone(), e))?;

        tracing::trace!("Created file");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf: _,
            force: _,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Delete file `{}`", path.display()),
                vec![format!("Delete file `{}`", path.display())],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
        user = self.user,
        group = self.group,
        mode = self.mode.map(|v| tracing::field::display(format!("{:#o}", v))),
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
