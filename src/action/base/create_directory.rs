use std::fs;
use std::io::ErrorKind;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use nix::unistd::{chown, fchown, Gid, Group, Uid, User};

use target_lexicon::OperatingSystem;
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{Action, ActionDescription, ActionErrorKind, ActionState};
use crate::action::{ActionError, StatefulAction};
use crate::execute_command;
use crate::util::OnMissing;

use super::{validate_no_extended_acl, PublishedPathIdentity};

/** Create a directory at the given location, optionally with an owning user, group, and mode.

If `force_prune_on_revert` is set, the folder will always be deleted on
[`revert`](CreateDirectory::revert).
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_directory")]
pub struct CreateDirectory {
    pub(crate) path: PathBuf,
    pub(crate) user: Option<String>,
    pub(crate) group: Option<String>,
    pub(crate) mode: Option<u32>,
    pub(crate) is_mountpoint: bool,
    pub(crate) force_prune_on_revert: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) shared: Option<SharedDirectory>,
}

#[derive(Debug, Default, serde::Deserialize, serde::Serialize, Clone)]
pub(crate) struct SharedDirectory {
    identity: Option<PublishedPathIdentity>,
}

impl CreateDirectory {
    pub(crate) fn plan_shared(
        path: impl Into<PathBuf>,
        user: Option<String>,
        group: Option<String>,
        mode: u32,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.into();
        let existing = match fs::symlink_metadata(&path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => {
                return Err(Self::error(ActionErrorKind::GettingMetadata(path, error)));
            },
        };
        if let Some(metadata) = &existing {
            if !metadata.is_dir() {
                return Err(Self::error(ActionErrorKind::PathWasNotDirectory(path)));
            }
            let directory = Self::open_shared(&path)?;
            let metadata = directory.metadata().map_err(|error| {
                Self::error(ActionErrorKind::GettingMetadata(path.clone(), error))
            })?;
            if let Some(uid) = Self::resolve_uid(user.as_deref())? {
                if metadata.uid() != uid.as_raw() {
                    return Err(Self::error(ActionErrorKind::PathUserMismatch(
                        path,
                        metadata.uid(),
                        uid.as_raw(),
                    )));
                }
            }
            let permissions = metadata.mode() & 0o777;
            if permissions & 0o022 != 0 {
                return Err(Self::error(ActionErrorKind::PathModeMismatch(
                    path,
                    permissions,
                    permissions & !0o022,
                )));
            }
            validate_no_extended_acl(&directory, &path).map_err(Self::error)?;
        }
        let action = Self {
            path,
            user,
            group,
            mode: Some(mode),
            is_mountpoint: false,
            force_prune_on_revert: false,
            shared: Some(SharedDirectory::default()),
        };
        Ok(if existing.is_some() {
            StatefulAction::completed(action)
        } else {
            StatefulAction::uncompleted(action)
        })
    }

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
                shared: None,
            },
            state: action_state,
        })
    }

    pub(crate) fn use_shared_directory_policy(&mut self) {
        self.shared.get_or_insert_with(SharedDirectory::default);
    }

    pub(crate) fn has_shared_identity(&self) -> bool {
        self.shared
            .as_ref()
            .is_some_and(|shared| shared.identity.is_some())
    }

    pub(crate) fn owns_shared_path(&self) -> Result<bool, ActionError> {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) => Ok(self.matches_shared_identity(&metadata)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(Self::error(ActionErrorKind::GettingMetadata(
                self.path.clone(),
                error,
            ))),
        }
    }

    fn matches_shared_identity(&self, metadata: &fs::Metadata) -> bool {
        metadata.is_dir()
            && self
                .shared
                .as_ref()
                .and_then(|shared| shared.identity.as_ref())
                .is_some_and(|identity| *identity == PublishedPathIdentity::from(metadata))
    }

    fn resolve_uid(user: Option<&str>) -> Result<Option<Uid>, ActionError> {
        let Some(user) = user else { return Ok(None) };
        let entry = User::from_name(user)
            .map_err(|error| Self::error(ActionErrorKind::GettingUserId(user.into(), error)))?
            .ok_or_else(|| Self::error(ActionErrorKind::NoUser(user.into())))?;
        Ok(Some(entry.uid))
    }

    fn open_shared(path: &Path) -> Result<fs::File, ActionError> {
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_DIRECTORY | nix::libc::O_NOFOLLOW)
            .open(path)
            .map_err(|error| Self::error(ActionErrorKind::Open(path.into(), error)))
    }

    fn create_shared(&mut self, uid: Option<Uid>, gid: Option<Gid>) -> Result<(), ActionError> {
        let path = &self.path;
        let resuming = self.has_shared_identity();
        if !resuming {
            // An existing entry belongs to someone else, even if it appeared after planning.
            fs::DirBuilder::new()
                .mode(0o700)
                .create(path)
                .map_err(|error| {
                    Self::error(ActionErrorKind::CreateDirectory(path.clone(), error))
                })?;
        }
        let directory = Self::open_shared(path)?;
        let metadata = directory
            .metadata()
            .map_err(|error| Self::error(ActionErrorKind::GettingMetadata(path.clone(), error)))?;
        if resuming && !self.matches_shared_identity(&metadata) {
            return Err(Self::error(ActionErrorKind::DirExists(path.clone())));
        }
        // Record ownership before setting attributes so partial installs can clean up.
        self.shared = Some(SharedDirectory {
            identity: Some(PublishedPathIdentity::from(&metadata)),
        });
        validate_no_extended_acl(&directory, path).map_err(Self::error)?;
        fchown(directory.as_raw_fd(), uid, gid)
            .map_err(|error| Self::error(ActionErrorKind::Chown(path.clone(), error)))?;
        if let Some(mode) = self.mode {
            directory
                .set_permissions(PermissionsExt::from_mode(mode))
                .map_err(|error| {
                    Self::error(ActionErrorKind::SetPermissions(mode, path.clone(), error))
                })?;
        }
        Ok(())
    }

    fn revert_shared(&self, shared: &SharedDirectory) -> Result<(), ActionError> {
        // Older receipts lack identity information. Leave the shared path in place.
        let Some(identity) = &shared.identity else {
            return Ok(());
        };
        let path = &self.path;
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error)
                if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
            {
                return Ok(());
            },
            Err(error) => {
                return Err(Self::error(ActionErrorKind::GettingMetadata(
                    path.clone(),
                    error,
                )));
            },
        };
        if !metadata.is_dir() || *identity != PublishedPathIdentity::from(&metadata) {
            return Ok(());
        }
        // rmdir checks emptiness and removes the directory in one operation.
        match fs::remove_dir(path) {
            Ok(()) => Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty | ErrorKind::NotADirectory
                ) =>
            {
                Ok(())
            },
            Err(error) => Err(Self::error(ActionErrorKind::Remove(path.clone(), error))),
        }
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
            shared,
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
        let uid = Self::resolve_uid(user.as_deref())?;

        if shared.is_some() {
            return self.create_shared(uid, gid);
        }

        tokio::fs::create_dir_all(&path)
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
        if self.shared.is_some() {
            return if self.has_shared_identity() {
                vec![ActionDescription::new(
                    format!(
                        "Remove the directory `{}` if it is empty and still matches the installer record",
                        self.path.display(),
                    ),
                    vec![],
                )]
            } else {
                vec![]
            };
        }
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            is_mountpoint,
            force_prune_on_revert,
            shared: _,
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
        if let Some(shared) = &self.shared {
            self.revert_shared(shared)?;
            self.shared = Some(SharedDirectory::default());
            return Ok(());
        }
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            is_mountpoint,
            force_prune_on_revert,
            shared: _,
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
                        crate::util::remove_dir_all(&child_path_path, OnMissing::Error)
                            .await
                            .map_err(|e| ActionErrorKind::Remove(path.clone(), e))
                            .map_err(Self::error)?
                    } else {
                        crate::util::remove_file(&child_path_path, OnMissing::Error)
                            .await
                            .map_err(|e| ActionErrorKind::Remove(path.clone(), e))
                            .map_err(Self::error)?
                    }
                }
            },
            (true, _, false) => {
                tracing::debug!("Not cleaning mountpoint `{}`", path.display());
            },
            (false, true, _) | (false, false, true) => {
                crate::util::remove_dir_all(path, OnMissing::Error)
                    .await
                    .map_err(|e| ActionErrorKind::Remove(path.clone(), e))
                    .map_err(Self::error)?
            },
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

    let mut mount_command = match OperatingSystem::host() {
        OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => {
            let mut cmd = Command::new("/sbin/mount");
            cmd.arg("-d"); // `-d` means `--dry-run`
            cmd
        },
        _ => {
            let mut cmd = Command::new("mount");
            cmd.arg("-f"); // `-f` means `--fake` not `--force`
            cmd
        },
    };

    mount_command.process_group(0);

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

        let split_token = match OperatingSystem::host() {
            // Each line on MacOS looks like `/dev/disk3s6 on /System/Volumes/VM (apfs, local, noexec, journaled, noatime, nobrowse)`
            OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => "(",
            // Each line on Linux looks like `portal on /run/user/1000/doc type fuse.portal (rw,nosuid,nodev,relatime,user_id=1000,group_id=100)`
            _ => "type",
        };

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
