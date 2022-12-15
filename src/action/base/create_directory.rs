use std::os::unix::prelude::PermissionsExt;
use std::path::{Path, PathBuf};

use nix::unistd::{chown, Group, User};

use tokio::fs::{create_dir, remove_dir_all};
use tracing::{span, Span};

use crate::action::{Action, ActionDescription, ActionState};
use crate::action::{ActionError, StatefulAction};

/** Create a directory at the given location, optionally with an owning user, group, and mode.

If `force_prune_on_revert` is set, the folder will always be deleted on
[`revert`](CreateDirectory::revert).
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateDirectory {
    path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    force_prune_on_revert: bool,
}

impl CreateDirectory {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        force_prune_on_revert: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref();
        let user = user.into();
        let group = group.into();
        let mode = mode.into();

        let action_state = if path.exists() {
            let metadata = tokio::fs::metadata(path)
                .await
                .map_err(|e| ActionError::GettingMetadata(path.to_path_buf(), e))?;
            if metadata.is_dir() || metadata.is_symlink() {
                tracing::debug!(
                    "Creating directory `{}` already complete, skipping",
                    path.display(),
                );
                // TODO: Validate owner/group...
                ActionState::Skipped
            } else {
                return Err(ActionError::Exists(path.to_owned()));
            }
        } else {
            ActionState::Uncompleted
        };

        Ok(StatefulAction {
            action: Self {
                path: path.to_path_buf(),
                user,
                group,
                mode,
                force_prune_on_revert,
            },
            state: action_state,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_directory")]
impl Action for CreateDirectory {
    fn tracing_synopsis(&self) -> String {
        format!("Create directory `{}`", self.path.display())
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "create_directory",
            path = tracing::field::display(self.path.display()),
            user = self.user,
            group = self.group,
            mode = self
                .mode
                .map(|v| tracing::field::display(format!("{:#o}", v))),
        )
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
            force_prune_on_revert: _,
        } = self;

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

        create_dir(path.clone())
            .await
            .map_err(|e| ActionError::CreateDirectory(path.clone(), e))?;
        chown(path, uid, gid).map_err(|e| ActionError::Chown(path.clone(), e))?;

        if let Some(mode) = mode {
            tokio::fs::set_permissions(&path, PermissionsExt::from_mode(*mode))
                .await
                .map_err(|e| ActionError::SetPermissions(*mode, path.to_owned(), e))?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            force_prune_on_revert,
        } = &self;
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

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            force_prune_on_revert,
        } = self;

        let is_empty = path
            .read_dir()
            .map_err(|e| ActionError::Read(path.clone(), e))?
            .next()
            .is_some();
        match (is_empty, force_prune_on_revert) {
            (true, _) | (false, true) => remove_dir_all(path.clone())
                .await
                .map_err(|e| ActionError::Remove(path.clone(), e))?,
            (false, false) => {
                tracing::debug!("Not removing `{}`, the folder is not empty", path.display());
            },
        };

        Ok(())
    }
}
