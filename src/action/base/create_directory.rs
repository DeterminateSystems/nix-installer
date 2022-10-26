use std::os::unix::prelude::PermissionsExt;
use std::path::{Path, PathBuf};

use nix::unistd::{chown, Group, User};

use tokio::fs::{create_dir, remove_dir_all};

use crate::action::{Action, ActionDescription, ActionError, ActionState};

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
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = path.as_ref();
        let user = user.into();
        let group = group.into();
        let mode = mode.into();

        let action_state = if path.exists() {
            let metadata = tokio::fs::metadata(path).await.map_err(|e| {
                CreateDirectoryError::GettingMetadata(path.to_path_buf(), e).boxed()
            })?;
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
                ))
                .boxed());
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
#[typetag::serde(name = "create-directory")]
impl Action for CreateDirectory {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            force_prune_on_revert: _,
            action_state,
        } = &self;
        if *action_state == ActionState::Completed {
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
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
                    .map_err(|e| CreateDirectoryError::GroupId(group.clone(), e).boxed())?
                    .ok_or(CreateDirectoryError::NoGroup(group.clone()).boxed())?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| CreateDirectoryError::UserId(user.clone(), e).boxed())?
                    .ok_or(CreateDirectoryError::NoUser(user.clone()).boxed())?
                    .uid,
            )
        } else {
            None
        };

        create_dir(path.clone())
            .await
            .map_err(|e| CreateDirectoryError::Creating(path.clone(), e).boxed())?;
        chown(path, uid, gid).map_err(|e| CreateDirectoryError::Chown(path.clone(), e).boxed())?;

        if let Some(mode) = mode {
            tokio::fs::set_permissions(&path, PermissionsExt::from_mode(*mode))
                .await
                .map_err(|e| {
                    CreateDirectoryError::SetPermissions(*mode, path.to_owned(), e).boxed()
                })?;
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
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
            .map_err(|e| CreateDirectoryError::ReadDir(path.clone(), e).boxed())?
            .next()
            .is_some();
        match (is_empty, force_prune_on_revert) {
            (true, _) | (false, true) => remove_dir_all(path.clone())
                .await
                .map_err(|e| CreateDirectoryError::Removing(path.clone(), e).boxed())?,
            (false, false) => {},
        };

        tracing::trace!("Removed directory");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateDirectoryError {
    #[error(transparent)]
    Exists(std::io::Error),
    #[error("Creating directory `{0}`")]
    Creating(std::path::PathBuf, #[source] std::io::Error),
    #[error("Removing directory `{0}`")]
    Removing(std::path::PathBuf, #[source] std::io::Error),
    #[error("Getting metadata for {0}`")]
    GettingMetadata(std::path::PathBuf, #[source] std::io::Error),
    #[error("Reading directory `{0}``")]
    ReadDir(std::path::PathBuf, #[source] std::io::Error),
    #[error("Set mode `{0}` on `{1}`")]
    SetPermissions(u32, std::path::PathBuf, #[source] std::io::Error),
    #[error("Chowning directory `{0}`")]
    Chown(std::path::PathBuf, #[source] nix::errno::Errno),
    #[error("Getting uid for user `{0}`")]
    UserId(String, #[source] nix::errno::Errno),
    #[error("Getting user `{0}`")]
    NoUser(String),
    #[error("Getting gid for group `{0}`")]
    GroupId(String, #[source] nix::errno::Errno),
    #[error("Getting group `{0}`")]
    NoGroup(String),
}
