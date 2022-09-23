use std::path::{Path, PathBuf};

use nix::unistd::{chown, Group, User};
use serde::Serialize;
use tokio::fs::create_dir;

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, ActionState, Action, ActionError};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateDirectory {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
    action_state: ActionState,
}

impl CreateDirectory {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: String,
        group: String,
        mode: u32,
        force: bool,
    ) -> Result<Self, CreateDirectoryError> {
        let path = path.as_ref();

        if path.exists() && !force {
            return Err(CreateDirectoryError::Exists(
                path.to_path_buf(),
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("Directory `{}` already exists", path.display()),
                ),
            ));
        }

        Ok(Self {
            path: path.to_path_buf(),
            user,
            group,
            mode,
            action_state: ActionState::Planned,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for CreateDirectory {
    type Error = CreateDirectoryError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user,
            group,
            mode,
            action_state,
        } = &self;
        vec![ActionDescription::new(
            format!("Create the directory `{}`", path.display()),
            vec![format!(
                "Creating directory `{}` owned by `{user}:{group}` with mode `{mode:#o}`",
                path.display()
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
            action_state,
        } = self;

        let gid = Group::from_name(group.as_str())
            .map_err(|e| Self::Error::GroupId(group.clone(), e))?
            .ok_or(Self::Error::NoGroup(group.clone()))?
            .gid;
        let uid = User::from_name(user.as_str())
            .map_err(|e| Self::Error::UserId(user.clone(), e))?
            .ok_or(Self::Error::NoUser(user.clone()))?
            .uid;

        tracing::trace!(path = %path.display(), "Creating directory");
        create_dir(path.clone())
            .await
            .map_err(|e| Self::Error::Creating(path.clone(), e))?;
        chown(path, Some(uid), Some(gid)).map_err(|e| Self::Error::Chown(path.clone(), e))?;

        *action_state = ActionState::Completed;
        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}


impl From<CreateDirectory> for Action {
    fn from(v: CreateDirectory) -> Self {
        Action::CreateDirectory(v)
    }
}


#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateDirectoryError {
    #[error("Directory exists `{0}`")]
    Exists(std::path::PathBuf, #[source] #[serde(serialize_with = "crate::serialize_error_to_display")] std::io::Error),
    #[error("Creating directory `{0}`")]
    Creating(std::path::PathBuf, #[source] #[serde(serialize_with = "crate::serialize_error_to_display")] std::io::Error),
    #[error("Chowning directory `{0}`")]
    Chown(std::path::PathBuf, #[source] #[serde(serialize_with = "crate::serialize_error_to_display")] nix::errno::Errno),
    #[error("Getting uid for user `{0}`")]
    UserId(String, #[source] #[serde(serialize_with = "crate::serialize_error_to_display")] nix::errno::Errno),
    #[error("Getting user `{0}`")]
    NoUser(String),
    #[error("Getting gid for group `{0}`")]
    GroupId(String, #[source] #[serde(serialize_with = "crate::serialize_error_to_display")] nix::errno::Errno),
    #[error("Getting group `{0}`")]
    NoGroup(String),
}
