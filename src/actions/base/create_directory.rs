use std::os::unix::prelude::PermissionsExt;
use std::path::{Path, PathBuf};

use nix::unistd::{chown, Group, User};
use serde::Serialize;
use tokio::fs::{create_dir, remove_dir_all};

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

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
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for CreateDirectory {
    type Error = CreateDirectoryError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user,
            group,
            mode,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Create the directory `{}`", path.display()),
                vec![format!(
                    "Creating directory `{}` owned by `{user}:{group}` with mode `{mode:#o}`",
                    path.display()
                )],
            )]
        }
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
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating directory");
            return Ok(());
        }
        tracing::debug!("Creating directory");

        let gid = Group::from_name(group.as_str())
            .map_err(|e| Self::Error::GroupId(group.clone(), e))?
            .ok_or(Self::Error::NoGroup(group.clone()))?
            .gid;
        let uid = User::from_name(user.as_str())
            .map_err(|e| Self::Error::UserId(user.clone(), e))?
            .ok_or(Self::Error::NoUser(user.clone()))?
            .uid;

        create_dir(path.clone())
            .await
            .map_err(|e| Self::Error::Creating(path.clone(), e))?;
        chown(path, Some(uid), Some(gid)).map_err(|e| Self::Error::Chown(path.clone(), e))?;

        tokio::fs::set_permissions(&path, PermissionsExt::from_mode(*mode))
            .await
            .map_err(|e| Self::Error::SetPermissions(*mode, path.to_owned(), e))?;

        tracing::trace!("Created directory");
        *action_state = ActionState::Completed;
        Ok(())
    }


    fn describe_revert(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Remove the directory `{}`", path.display()),
                vec![],
            )]
        }
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
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Removing directory");
            return Ok(());
        }
        tracing::debug!("Removing directory");

        tracing::trace!(path = %path.display(), "Removing directory");
        remove_dir_all(path.clone())
            .await
            .map_err(|e| Self::Error::Removing(path.clone(), e))?;

        tracing::trace!("Removed directory");
        *action_state = ActionState::Uncompleted;
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
    Exists(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Creating directory `{0}`")]
    Creating(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Removing directory `{0}`")]
    Removing(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
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
}
