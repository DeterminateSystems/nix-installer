use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use nix::unistd::{chown, Group, User};

use tokio::fs::{create_dir, remove_dir_all, remove_file};
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{Action, ActionDescription, ActionErrorKind, ActionState};
use crate::action::{ActionError, StatefulAction};
use crate::execute_command;

/** Create a directory at the given location, optionally with an owning user, group, and mode.

If `force_prune_on_revert` is set, the folder will always be deleted on
[`revert`](CreateDirectory::revert).
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_directory")]
pub struct CreateDirectory {
    path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    is_mountpoint: bool,
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
        let mut is_mountpoint = false;

        let action_state = if path.exists() {
            let metadata = tokio::fs::metadata(&path)
                .await
                .map_err(|e| ActionErrorKind::GettingMetadata(path.clone(), e))
                .map_err(Self::error)?;
            if !metadata.is_dir() {
                return Err(Self::error(ActionErrorKind::PathWasNotDirectory(
                    path.to_owned(),
                )));
            }

            // Does it have the right user/group?
            if let Some(user) = &user {
                // If the file exists, the user must also exist to be correct.
                let expected_uid = User::from_name(user.as_str())
                    .map_err(|e| ActionErrorKind::GettingUserId(user.clone(), e))
                    .map_err(Self::error)?
                    .ok_or_else(|| ActionErrorKind::NoUser(user.clone()))
                    .map_err(Self::error)?
                    .uid;
                let found_uid = metadata.uid();
                if found_uid != expected_uid.as_raw() {
                    return Err(Self::error(ActionErrorKind::PathUserMismatch(
                        path.clone(),
                        found_uid,
                        expected_uid.as_raw(),
                    )));
                }
            }
            if let Some(group) = &group {
                // If the file exists, the group must also exist to be correct.
                let expected_gid = Group::from_name(group.as_str())
                    .map_err(|e| ActionErrorKind::GettingGroupId(group.clone(), e))
                    .map_err(Self::error)?
                    .ok_or_else(|| ActionErrorKind::NoUser(group.clone()))
                    .map_err(Self::error)?
                    .gid;
                let found_gid = metadata.gid();
                if found_gid != expected_gid.as_raw() {
                    return Err(Self::error(ActionErrorKind::PathGroupMismatch(
                        path.clone(),
                        found_gid,
                        expected_gid.as_raw(),
                    )));
                }
            }

            // Is it a mountpoint?
            is_mountpoint = path_is_mountpoint(&path).await.map_err(Self::error)?;
            tracing::debug!(
                is_mountpoint,
                "Creating directory `{}` already complete",
                path.display(),
            );
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
                is_mountpoint,
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
            is_mountpoint, // If `is_mountpoint = true` the `ActionState` should be completed.
            force_prune_on_revert: _,
        } = self;

        if *is_mountpoint {
            // A `/nix` mount exists, we don't need to do anything.
            return Ok(());
        }

        let gid = if let Some(group) = group {
            Some(
                Group::from_name(group.as_str())
                    .map_err(|e| ActionErrorKind::GettingGroupId(group.clone(), e))
                    .map_err(Self::error)?
                    .ok_or(ActionErrorKind::NoGroup(group.clone()))
                    .map_err(Self::error)?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| ActionErrorKind::GettingUserId(user.clone(), e))
                    .map_err(Self::error)?
                    .ok_or(ActionErrorKind::NoUser(user.clone()))
                    .map_err(Self::error)?
                    .uid,
            )
        } else {
            None
        };

        create_dir(&path)
            .await
            .map_err(|e| ActionErrorKind::CreateDirectory(path.clone(), e))
            .map_err(Self::error)?;
        chown(path, uid, gid)
            .map_err(|e| ActionErrorKind::Chown(path.clone(), e))
            .map_err(Self::error)?;

        if let Some(mode) = mode {
            tokio::fs::set_permissions(&path, PermissionsExt::from_mode(*mode))
                .await
                .map_err(|e| ActionErrorKind::SetPermissions(*mode, path.to_owned(), e))
                .map_err(Self::error)?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            is_mountpoint,
            force_prune_on_revert,
        } = &self;
        match (is_mountpoint, force_prune_on_revert) {
            (true, true) => vec![ActionDescription::new(
                format!("Clean contents of mountpoint `{}`", path.display(),),
                vec![],
            )],
            (true, false) => vec![],
            (false, _) => vec![ActionDescription::new(
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
            )],
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            is_mountpoint,
            force_prune_on_revert,
        } = self;

        let contents = path
            .read_dir()
            .map_err(|e| ActionErrorKind::Read(path.clone(), e))
            .map_err(Self::error)?
            .collect::<Vec<_>>();
        let is_empty = contents.is_empty();

        match (is_mountpoint, is_empty, force_prune_on_revert) {
            (true, _, true) => {
                tracing::debug!("Cleaning mountpoint `{}`", path.display());
                for child_path in contents {
                    let child_path = child_path
                        .map_err(|e| ActionErrorKind::ReadDir(path.clone(), e))
                        .map_err(Self::error)?;
                    let child_path_path = child_path.path();
                    let child_path_type = child_path
                        .file_type()
                        .map_err(|e| ActionErrorKind::GettingMetadata(child_path_path.clone(), e))
                        .map_err(Self::error)?;
                    if child_path_type.is_dir() {
                        remove_dir_all(child_path_path.clone())
                            .await
                            .map_err(|e| ActionErrorKind::Remove(path.clone(), e))
                            .map_err(Self::error)?
                    } else {
                        remove_file(child_path_path)
                            .await
                            .map_err(|e| ActionErrorKind::Remove(path.clone(), e))
                            .map_err(Self::error)?
                    }
                }
            },
            (true, _, false) => {
                tracing::debug!("Not cleaning mountpoint `{}`", path.display());
            },
            (false, true, _) | (false, false, true) => remove_dir_all(path.clone())
                .await
                .map_err(|e| ActionErrorKind::Remove(path.clone(), e))
                .map_err(Self::error)?,
            (false, false, false) => {
                tracing::debug!("Not removing `{}`, the folder is not empty", path.display());
            },
        };

        Ok(())
    }
}

// There are cleaner ways of doing this (eg `systemctl status $PATH`) however we need a widely supported way.
async fn path_is_mountpoint(path: &Path) -> Result<bool, ActionErrorKind> {
    let path_str = match path.to_str() {
        Some(path_str) => path_str,
        None => return Err(ActionErrorKind::PathNoneString(path.to_path_buf())),
    };

    let mut mount_command = Command::new("mount");
    mount_command.process_group(0);

    #[cfg(target_os = "macos")]
    mount_command.arg("-d"); // `-d` means `--dry-run`
    #[cfg(target_os = "linux")]
    mount_command.arg("-f"); // `-f` means `--fake` not `--force`

    let output = execute_command(&mut mount_command).await?;
    let output_string = String::from_utf8(output.stdout).map_err(ActionErrorKind::FromUtf8)?;

    for line in output_string.lines() {
        let mut line_splitter = line.split(" on ");
        match line_splitter.next() {
            Some(_device) => (),
            None => continue,
        }
        let destination_and_options = match line_splitter.next() {
            Some(destination_and_options) => destination_and_options,
            None => continue,
        };
        // Each line on Linux looks like `portal on /run/user/1000/doc type fuse.portal (rw,nosuid,nodev,relatime,user_id=1000,group_id=100)`
        #[cfg(target_os = "linux")]
        let split_token = "type";
        // Each line on MacOS looks like `/dev/disk3s6 on /System/Volumes/VM (apfs, local, noexec, journaled, noatime, nobrowse)`
        #[cfg(target_os = "macos")]
        let split_token = "(";

        if let Some(mount_path) = destination_and_options.rsplit(split_token).last() {
            let trimmed = mount_path.trim();
            if trimmed == path_str {
                tracing::trace!("Found mountpoint for `{mount_path}`");
                return Ok(true);
            }
        }
    }

    Ok(false)
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
