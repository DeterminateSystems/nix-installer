use nix::unistd::{chown, Group, User};

use std::path::{Path, PathBuf};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::AsyncWriteExt,
};

use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

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
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = path.as_ref().to_path_buf();

        if path.exists() && !force {
            return Err(CreateFileError::Exists(path.to_path_buf()).boxed());
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
#[typetag::serde(name = "create_file")]
impl Action for CreateFile {
    fn tracing_synopsis(&self) -> String {
        format!("Create or overwrite file `{}`", self.path.display())
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
            force: _,
            action_state,
        } = self;

        let mut options = OpenOptions::new();
        options.create_new(true).write(true).read(true);

        if let Some(mode) = mode {
            options.mode(*mode);
        }

        let mut file = options
            .open(&path)
            .await
            .map_err(|e| CreateFileError::OpenFile(path.to_owned(), e).boxed())?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| CreateFileError::WriteFile(path.to_owned(), e).boxed())?;

        let gid = if let Some(group) = group {
            Some(
                Group::from_name(group.as_str())
                    .map_err(|e| CreateFileError::GroupId(group.clone(), e).boxed())?
                    .ok_or(CreateFileError::NoGroup(group.clone()).boxed())?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| CreateFileError::UserId(user.clone(), e).boxed())?
                    .ok_or(CreateFileError::NoUser(user.clone()).boxed())?
                    .uid,
            )
        } else {
            None
        };
        chown(path, uid, gid).map_err(|e| CreateFileError::Chown(path.clone(), e).boxed())?;

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
            action_state: _,
        } = &self;

        vec![ActionDescription::new(
            format!("Delete file `{}`", path.display()),
            vec![format!("Delete file `{}`", path.display())],
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
            buf: _,
            force: _,
            action_state,
        } = self;

        remove_file(&path)
            .await
            .map_err(|e| CreateFileError::RemoveFile(path.to_owned(), e).boxed())?;

        Ok(())
    }

    fn action_state(&self) -> ActionState {
        self.action_state
    }

    fn set_action_state(&mut self, action_state: ActionState) {
        self.action_state = action_state;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateFileError {
    #[error("File exists `{0}`")]
    Exists(std::path::PathBuf),
    #[error("Remove file `{0}`")]
    RemoveFile(std::path::PathBuf, #[source] std::io::Error),
    #[error("Open file `{0}`")]
    OpenFile(std::path::PathBuf, #[source] std::io::Error),
    #[error("Write file `{0}`")]
    WriteFile(std::path::PathBuf, #[source] std::io::Error),
    #[error("Getting uid for user `{0}`")]
    UserId(String, #[source] nix::errno::Errno),
    #[error("Getting user `{0}`")]
    NoUser(String),
    #[error("Getting gid for group `{0}`")]
    GroupId(String, #[source] nix::errno::Errno),
    #[error("Getting group `{0}`")]
    NoGroup(String),
    #[error("Chowning directory `{0}`")]
    Chown(std::path::PathBuf, #[source] nix::errno::Errno),
}
