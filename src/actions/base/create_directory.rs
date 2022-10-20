use std::os::unix::prelude::PermissionsExt;
use std::path::{Path, PathBuf};

use nix::unistd::{chown, Group, User};
use serde::Serialize;
use tokio::fs::{create_dir, remove_dir_all};

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateDirectory {
    path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    action_state: ActionState,
    force_prune_on_revert: bool,
}

impl CreateDirectory {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        force_prune_on_revert: bool,
    ) -> Result<Self, CreateDirectoryError> {
        let path = path.as_ref();
        let user = user.into();
        let group = group.into();
        let mode = mode.into();

        let action_state = if path.exists() {
            let metadata = tokio::fs::metadata(path)
                .await
                .map_err(|e| CreateDirectoryError::GettingMetadata(path.to_path_buf(), e))?;
            if metadata.is_dir() {
                // TODO: Validate owner/group...
                ActionState::Completed
            } else {
                return Err(CreateDirectoryError::Exists(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!(
                        "Path `{}` already exists and is not directory",
                        path.display()
                    ),
                )));
            }
        } else {
            ActionState::Uncompleted
        };

        Ok(Self {
            path: path.to_path_buf(),
            user,
            group,
            mode,
            force_prune_on_revert,
            action_state,
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
            force_prune_on_revert: _,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Create the directory `{}`", path.display()),
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
            force_prune_on_revert: _,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating directory");
            return Ok(());
        }
        tracing::debug!("Creating directory");

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

        create_dir(path.clone())
            .await
            .map_err(|e| Self::Error::Creating(path.clone(), e))?;
        chown(path, uid, gid).map_err(|e| Self::Error::Chown(path.clone(), e))?;

        if let Some(mode) = mode {
            tokio::fs::set_permissions(&path, PermissionsExt::from_mode(*mode))
                .await
                .map_err(|e| Self::Error::SetPermissions(*mode, path.to_owned(), e))?;
        }

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
            force_prune_on_revert,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!(
                    "Remove the directory `{}`{}",
                    path.display(),
                    if *force_prune_on_revert {
                        ""
                    } else {
                        " if no other contents exists"
                    }
                ),
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
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            force_prune_on_revert,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Removing directory");
            return Ok(());
        }
        tracing::debug!("Removing directory");

        tracing::trace!(path = %path.display(), "Removing directory");

        let is_empty = path
            .read_dir()
            .map_err(|e| CreateDirectoryError::ReadDir(path.clone(), e))?
            .next()
            .is_some();
        match (is_empty, force_prune_on_revert) {
            (true, _) | (false, true) => remove_dir_all(path.clone())
                .await
                .map_err(|e| Self::Error::Removing(path.clone(), e))?,
            (false, false) => {},
        };

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
    #[error(transparent)]
    Exists(#[serde(serialize_with = "crate::serialize_error_to_display")] std::io::Error),
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
    #[error("Getting metadata for {0}`")]
    GettingMetadata(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Reading directory `{0}``")]
    ReadDir(
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
