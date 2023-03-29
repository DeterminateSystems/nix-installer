use std::os::unix::fs::{MetadataExt, PermissionsExt};
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
        let path = path.as_ref().to_path_buf();
        let user = user.into();
        let group = group.into();
        let mode = mode.into();

        let action_state = if path.exists() {
            let metadata = tokio::fs::metadata(&path)
                .await
                .map_err(|e| ActionError::GettingMetadata(path.clone(), e))?;
            if !metadata.is_dir() {
                return Err(ActionError::PathWasNotDirectory(path.to_owned()));
            }

            // Does it have the right user/group?
            if let Some(user) = &user {
                // If the file exists, the user must also exist to be correct.
                let expected_uid = User::from_name(user.as_str())
                    .map_err(|e| ActionError::GettingUserId(user.clone(), e))?
                    .ok_or_else(|| ActionError::NoUser(user.clone()))?
                    .uid;
                let found_uid = metadata.uid();
                if found_uid != expected_uid.as_raw() {
                    return Err(ActionError::PathUserMismatch(
                        path.clone(),
                        found_uid,
                        expected_uid.as_raw(),
                    ));
                }
            }
            if let Some(group) = &group {
                // If the file exists, the group must also exist to be correct.
                let expected_gid = Group::from_name(group.as_str())
                    .map_err(|e| ActionError::GettingGroupId(group.clone(), e))?
                    .ok_or_else(|| ActionError::NoUser(group.clone()))?
                    .gid;
                let found_gid = metadata.gid();
                if found_gid != expected_gid.as_raw() {
                    return Err(ActionError::PathGroupMismatch(
                        path.clone(),
                        found_gid,
                        expected_gid.as_raw(),
                    ));
                }
            }

            tracing::debug!("Creating directory `{}` already complete", path.display(),);
            ActionState::Completed
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
    fn action_tag() -> crate::action::ActionTag {
        crate::action::ActionTag("create_directory")
    }
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
                    .map_err(|e| ActionError::GettingGroupId(group.clone(), e))?
                    .ok_or(ActionError::NoGroup(group.clone()))?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| ActionError::GettingUserId(user.clone(), e))?
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
    async fn revert(&mut self) -> Result<(), Vec<ActionError>> {
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
            .is_none();

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

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn creates_and_deletes_empty_directory() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_dir = temp_dir.path().join("creates_and_deletes_empty_directory");
        let mut action = CreateDirectory::plan(test_dir.clone(), None, None, None, false).await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_dir.exists(), "Folder should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn creates_and_deletes_populated_directory_if_prune_true() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_dir = temp_dir
            .path()
            .join("creates_and_deletes_populated_directory_if_prune_true");
        let mut action = CreateDirectory::plan(test_dir.clone(), None, None, None, true).await?;

        action.try_execute().await?;

        let stub_file = test_dir.as_path().join("stub");
        tokio::fs::write(stub_file, "More content").await?;

        action.try_revert().await?;

        assert!(!test_dir.exists(), "Folder should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn creates_and_leaves_populated_directory_if_prune_false() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_dir = temp_dir
            .path()
            .join("creates_and_leaves_populated_directory_if_prune_false");
        let mut action = CreateDirectory::plan(test_dir.clone(), None, None, None, false).await?;

        action.try_execute().await?;

        let stub_file = test_dir.as_path().join("stub");
        tokio::fs::write(&stub_file, "More content").await?;

        action.try_revert().await?;

        assert!(test_dir.exists(), "Folder should not have been deleted");
        assert!(stub_file.exists(), "Folder should not have been deleted");

        Ok(())
    }
}
