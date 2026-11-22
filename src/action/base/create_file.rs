use nix::unistd::{chown, fchown, Gid, Group, Uid, User};
use tracing::{span, Span};

use std::{
    fs as std_fs,
    io::{ErrorKind, Read, Write},
    os::fd::AsRawFd,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::{
    action::{Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction},
    util::OnMissing,
};

use super::{validate_no_extended_acl, PublishedPathIdentity};

/** Create a file at the given location with the provided `buf`,
optionally with an owning user, group, and mode.

If `force` is set, the file will always be overwritten (and deleted)
regardless of its presence prior to install.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_file")]
pub struct CreateFile {
    pub(crate) path: PathBuf,
    pub(crate) user: Option<String>,
    pub(crate) group: Option<String>,
    mode: Option<u32>,
    buf: String,
    force: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    atomic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    published_identity: Option<PublishedPathIdentity>,
}

pub(crate) struct FileSnapshot {
    // Keep the inode from being reused while the caller checks launchd.
    _file: std_fs::File,
    metadata: std_fs::Metadata,
}

impl CreateFile {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        buf: String,
        force: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref().to_path_buf();
        let mode = mode.into();
        let user = user.into();
        let group = group.into();
        let this = Self {
            path,
            user,
            group,
            mode,
            buf,
            force,
            atomic: false,
            published_identity: None,
        };

        if this.path.exists() {
            // If the path exists, perhaps we can just skip this
            let mut file = File::open(&this.path)
                .await
                .map_err(|e| ActionErrorKind::Open(this.path.clone(), e))
                .map_err(Self::error)?;

            let metadata = file
                .metadata()
                .await
                .map_err(|e| ActionErrorKind::GettingMetadata(this.path.clone(), e))
                .map_err(Self::error)?;

            // Preserve the permission policy used by ordinary CreateFile actions.
            this.validate_metadata(&metadata, 0o777)?;

            // Does it have the right content?
            let mut discovered_buf = String::new();
            file.read_to_string(&mut discovered_buf)
                .await
                .map_err(|e| ActionErrorKind::Read(this.path.clone(), e))
                .map_err(Self::error)?;

            this.validate_contents(discovered_buf.as_bytes())?;

            tracing::debug!("Creating file `{}` already complete", this.path.display());
            return Ok(StatefulAction::completed(this));
        }

        Ok(StatefulAction::uncompleted(this))
    }

    fn validate_metadata(
        &self,
        metadata: &std_fs::Metadata,
        mode_mask: u32,
    ) -> Result<(), ActionError> {
        if !metadata.is_file() {
            return Err(Self::error(ActionErrorKind::PathWasNotFile(
                self.path.clone(),
            )));
        }

        if let Some(mode) = self.mode {
            // Does the file have the right permissions?
            let discovered_mode = metadata.permissions().mode();
            let discovered_mode = discovered_mode & mode_mask;

            if discovered_mode != mode {
                return Err(Self::error(ActionErrorKind::PathModeMismatch(
                    self.path.clone(),
                    discovered_mode,
                    mode,
                )));
            }
        }

        // Does it have the right user/group?
        if let Some(user) = &self.user {
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
                    self.path.clone(),
                    found_uid,
                    expected_uid.as_raw(),
                )));
            }
        }
        if let Some(group) = &self.group {
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
                    self.path.clone(),
                    found_gid,
                    expected_gid.as_raw(),
                )));
            }
        }

        Ok(())
    }

    fn validate_contents(&self, contents: &[u8]) -> Result<(), ActionError> {
        if contents != self.buf.as_bytes() {
            return Err(Self::error(ActionErrorKind::DifferentContent(
                self.path.clone(),
            )));
        }
        Ok(())
    }

    pub(crate) fn plan_atomically(
        path: impl Into<PathBuf>,
        user: Option<String>,
        group: Option<String>,
        mode: u32,
        buf: String,
    ) -> Result<StatefulAction<Self>, ActionError> {
        Self {
            path: path.into(),
            user,
            group,
            mode: Some(mode),
            buf,
            force: false,
            atomic: true,
            published_identity: None,
        }
        .check_atomic_plan()
    }

    pub(crate) fn replan_atomically(&self) -> Result<StatefulAction<Self>, ActionError> {
        let mut action = self.clone();
        action.atomic = true;
        action.check_atomic_plan()
    }

    fn check_atomic_plan(mut self) -> Result<StatefulAction<Self>, ActionError> {
        if self.open_matching_file()?.is_some() {
            Ok(StatefulAction::completed(self))
        } else {
            self.published_identity = None;
            Ok(StatefulAction::uncompleted(self))
        }
    }

    /// Publish a small file only after its contents and permissions are ready.
    /// Fail if the destination appeared since planning.
    fn create_atomically(&mut self) -> Result<(), ActionError> {
        let (uid, gid) = self.owner_ids()?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)
            .map_err(|error| Self::error(ActionErrorKind::Open(self.path.clone(), error)))?;
        temporary
            .write_all(self.buf.as_bytes())
            .map_err(|error| Self::error(ActionErrorKind::Write(self.path.clone(), error)))?;
        fchown(temporary.as_file().as_raw_fd(), uid, gid)
            .map_err(|error| Self::error(ActionErrorKind::Chown(self.path.clone(), error)))?;
        if let Some(mode) = self.mode {
            temporary
                .as_file()
                .set_permissions(PermissionsExt::from_mode(mode))
                .map_err(|error| {
                    Self::error(ActionErrorKind::SetPermissions(
                        mode,
                        self.path.clone(),
                        error,
                    ))
                })?;
        }
        validate_no_extended_acl(temporary.as_file(), &self.path).map_err(Self::error)?;
        let metadata = temporary.as_file().metadata().map_err(|error| {
            Self::error(ActionErrorKind::GettingMetadata(self.path.clone(), error))
        })?;
        let temporary_path = temporary.path().to_owned();
        temporary.persist_noclobber(&self.path).map_err(|error| {
            Self::error(ActionErrorKind::Rename(
                temporary_path,
                self.path.clone(),
                error.error,
            ))
        })?;
        self.published_identity = Some(PublishedPathIdentity::from(&metadata));
        Ok(())
    }

    fn path_metadata(&self) -> Result<Option<std_fs::Metadata>, ActionError> {
        match std_fs::symlink_metadata(&self.path) {
            Ok(metadata) => Ok(Some(metadata)),
            Err(error)
                if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
            {
                Ok(None)
            },
            Err(error) => Err(Self::error(ActionErrorKind::GettingMetadata(
                self.path.clone(),
                error,
            ))),
        }
    }

    fn open_matching_file(&self) -> Result<Option<FileSnapshot>, ActionError> {
        let Some(metadata) = self.path_metadata()? else {
            return Ok(None);
        };
        // Both installation and guarded removal check the full permission mode.
        self.validate_metadata(&metadata, 0o7777)?;
        // Avoid following a replacement symlink or blocking on a replacement FIFO.
        let mut file = match std_fs::OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(error)
                if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) =>
            {
                return Ok(None);
            },
            Err(error) if error.raw_os_error() == Some(nix::libc::ELOOP) => {
                return Err(Self::error(ActionErrorKind::PathWasNotFile(
                    self.path.clone(),
                )));
            },
            Err(error) => return Err(Self::error(ActionErrorKind::Open(self.path.clone(), error))),
        };
        let opened = file.metadata().map_err(|error| {
            Self::error(ActionErrorKind::GettingMetadata(self.path.clone(), error))
        })?;
        if !unchanged_file(&metadata, &opened) {
            return Ok(None);
        }
        validate_no_extended_acl(&file, &self.path).map_err(Self::error)?;
        let mut contents = Vec::new();
        (&mut file)
            .take(self.buf.len() as u64 + 1)
            .read_to_end(&mut contents)
            .map_err(|error| Self::error(ActionErrorKind::Read(self.path.clone(), error)))?;
        self.validate_contents(&contents)?;
        Ok(Some(FileSnapshot {
            _file: file,
            metadata: opened,
        }))
    }

    pub(crate) fn has_publication(&self, legacy_receipt_owns_file: bool) -> bool {
        // Early hook receipts recorded ownership without a file identity.
        self.published_identity.is_some() || legacy_receipt_owns_file
    }

    pub(crate) fn removal_candidate(
        &self,
        legacy_receipt_owns_file: bool,
    ) -> Result<Option<FileSnapshot>, ActionError> {
        if !self.has_publication(legacy_receipt_owns_file) {
            return Ok(None);
        }
        Ok(self.open_removable_file()?.filter(|snapshot| {
            self.published_identity
                .as_ref()
                .is_none_or(|identity| *identity == PublishedPathIdentity::from(&snapshot.metadata))
        }))
    }

    fn open_removable_file(&self) -> Result<Option<FileSnapshot>, ActionError> {
        match self.open_matching_file() {
            Ok(Some(snapshot)) if !is_unlink_protected(&snapshot.metadata) => Ok(Some(snapshot)),
            Ok(_) => Ok(None),
            Err(error)
                if matches!(
                    error.kind(),
                    ActionErrorKind::PathWasNotFile(_)
                        | ActionErrorKind::PathModeMismatch(..)
                        | ActionErrorKind::PathUserMismatch(..)
                        | ActionErrorKind::PathGroupMismatch(..)
                        | ActionErrorKind::UnexpectedAcl(_)
                        | ActionErrorKind::DifferentContent(_)
                ) =>
            {
                Ok(None)
            },
            Err(error) => Err(error),
        }
    }

    fn remove_unchanged_file(&self, original: FileSnapshot) -> Result<(), ActionError> {
        // The original snapshot was already checked against the publication record.
        let Some(current) = self.open_removable_file()? else {
            return Ok(());
        };
        // An independent writer can still change the pathname between the last
        // metadata check and unlink. Keep both operations synchronous.
        if !unchanged_file(&original.metadata, &current.metadata)
            || !self
                .path_metadata()?
                .is_some_and(|metadata| unchanged_file(&current.metadata, &metadata))
        {
            return Ok(());
        }
        match std_fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(Self::error(ActionErrorKind::Remove(
                self.path.clone(),
                error,
            ))),
        }
    }

    fn owner_ids(&self) -> Result<(Option<Uid>, Option<Gid>), ActionError> {
        let gid = if let Some(ref group) = self.group {
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
        let uid = if let Some(ref user) = self.user {
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
        Ok((uid, gid))
    }
}

fn unchanged_file(a: &std_fs::Metadata, b: &std_fs::Metadata) -> bool {
    (a.dev(), a.ino(), a.ctime(), a.ctime_nsec()) == (b.dev(), b.ino(), b.ctime(), b.ctime_nsec())
}

#[cfg(target_os = "macos")]
fn is_unlink_protected(metadata: &std_fs::Metadata) -> bool {
    use nix::libc::{SF_APPEND, SF_IMMUTABLE, UF_APPEND, UF_IMMUTABLE};
    use std::os::macos::fs::MetadataExt as _;

    metadata.st_flags() & (SF_APPEND | SF_IMMUTABLE | UF_APPEND | UF_IMMUTABLE) != 0
}

#[cfg(not(target_os = "macos"))]
fn is_unlink_protected(_: &std_fs::Metadata) -> bool {
    false
}

impl StatefulAction<CreateFile> {
    pub(crate) async fn try_revert_unchanged(
        &mut self,
        snapshot: FileSnapshot,
    ) -> Result<(), ActionError> {
        self.try_revert_with(|file| {
            Box::pin(async move {
                file.remove_unchanged_file(snapshot)?;
                file.published_identity = None;
                Ok(())
            })
        })
        .await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_file")]
impl Action for CreateFile {
    fn action_tag() -> ActionTag {
        ActionTag("create_file")
    }
    fn tracing_synopsis(&self) -> String {
        if self.atomic {
            format!("Create file `{}` only if it is absent", self.path.display())
        } else {
            format!("Create or overwrite file `{}`", self.path.display())
        }
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_file",
            path = tracing::field::display(self.path.display()),
            user = self.user,
            group = self.group,
            mode = self
                .mode
                .map(|v| tracing::field::display(format!("{:#o}", v))),
            buf = tracing::field::Empty,
        );

        if tracing::enabled!(tracing::Level::TRACE) {
            span.record("buf", &self.buf);
        }
        span
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        if tracing::enabled!(tracing::Level::TRACE) {
            let span = tracing::Span::current();
            span.record("buf", &self.buf);
        }

        if self.atomic {
            return self.create_atomically();
        }

        let mut options = OpenOptions::new();
        options.create_new(true).write(true).read(true);

        if let Some(mode) = self.mode {
            options.mode(mode);
        }

        let mut file = options
            .open(&self.path)
            .await
            .map_err(|e| ActionErrorKind::Open(self.path.to_owned(), e))
            .map_err(Self::error)?;

        file.write_all(self.buf.as_bytes())
            .await
            .map_err(|e| ActionErrorKind::Write(self.path.to_owned(), e))
            .map_err(Self::error)?;

        let (uid, gid) = self.owner_ids()?;
        chown(&self.path, uid, gid)
            .map_err(|e| ActionErrorKind::Chown(self.path.clone(), e))
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        if self.atomic {
            return if self.published_identity.is_some() {
                vec![ActionDescription::new(
                    format!(
                        "Delete file `{}` if it still matches the installer record",
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
            buf: _,
            force: _,
            atomic: _,
            published_identity: _,
        } = &self;

        vec![ActionDescription::new(
            format!("Delete file `{}`", path.display()),
            vec![format!("Delete file `{}`", path.display())],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        if self.atomic {
            if let Some(snapshot) = self.removal_candidate(false)? {
                self.remove_unchanged_file(snapshot)?;
            }
            self.published_identity = None;
            return Ok(());
        }
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf: _,
            force: _,
            atomic: _,
            published_identity: _,
        } = self;

        crate::util::remove_file(path, OnMissing::Ignore)
            .await
            .map_err(|e| ActionErrorKind::Remove(path.to_owned(), e))
            .map_err(Self::error)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use color_eyre::eyre::eyre;
    use tokio::fs::write;

    #[tokio::test]
    async fn atomic_revert_removes_only_the_published_file() -> eyre::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("helper");
        let plan = || CreateFile::plan_atomically(&path, None, None, 0o700, "helper".into());

        let mut owned = plan()?;
        assert!(owned.describe_execute()[0]
            .description
            .contains("only if it is absent"));
        owned.try_execute().await?;
        assert!(owned.inner().has_publication(false));
        let mut existing = plan()?;
        assert!(existing.describe_revert().is_empty());
        existing.try_revert().await?;
        assert!(path.exists());
        assert!(owned.describe_revert()[0]
            .description
            .contains("if it still matches"));
        owned.try_revert().await?;
        assert!(!owned.inner().has_publication(false));
        assert!(!path.exists());

        let mut owned = plan()?;
        owned.try_execute().await?;
        let replacement = dir.path().join("replacement");
        std_fs::copy(&path, &replacement)?;
        std_fs::rename(replacement, &path)?;
        owned.try_revert().await?;
        assert_eq!(std_fs::read_to_string(&path)?, "helper");
        Ok(())
    }

    #[tokio::test]
    async fn creates_and_deletes_file() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("creates_and_deletes_file");
        let mut action =
            CreateFile::plan(test_file.clone(), None, None, None, "Test".into(), false).await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn creates_and_deletes_file_even_if_edited() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir
            .path()
            .join("creates_and_deletes_file_even_if_edited");
        let mut action =
            CreateFile::plan(test_file.clone(), None, None, None, "Test".into(), false).await?;

        action.try_execute().await?;

        write(test_file.as_path(), "More content").await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_exact_files_and_reverts_them() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_exact_files_and_reverts_them");

        let test_content = "Some content";
        write(test_file.as_path(), test_content).await?;

        let mut action = CreateFile::plan(
            test_file.clone(),
            None,
            None,
            None,
            test_content.into(),
            false,
        )
        .await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_existing_different_files_and_errors() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir
            .path()
            .join("recognizes_existing_different_files_and_errors");

        write(test_file.as_path(), "Some content").await?;

        match CreateFile::plan(
            test_file.clone(),
            None,
            None,
            None,
            "Some different content".into(),
            false,
        )
        .await
        {
            Err(error) => match error.kind() {
                ActionErrorKind::DifferentContent(path) => assert_eq!(path, test_file.as_path()),
                _ => {
                    return Err(eyre!(
                        "Should have returned an ActionErrorKind::Exists error"
                    ))
                },
            },
            _ => {
                return Err(eyre!(
                    "Should have returned an ActionErrorKind::Exists error"
                ))
            },
        };

        assert!(test_file.exists(), "File should have not been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_wrong_mode_and_errors() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("recognizes_wrong_mode_and_errors");
        let initial_mode = 0o777;
        let expected_mode = 0o000;

        write(test_file.as_path(), "Some content").await?;
        tokio::fs::set_permissions(test_file.as_path(), PermissionsExt::from_mode(initial_mode))
            .await?;

        match CreateFile::plan(
            test_file.clone(),
            None,
            None,
            Some(expected_mode),
            "Some different content".into(),
            false,
        )
        .await
        {
            Err(err) => match err.kind() {
                ActionErrorKind::PathModeMismatch(path, got, expected) => {
                    assert_eq!(path, test_file.as_path());
                    assert_eq!(*expected, expected_mode);
                    assert_eq!(*got, initial_mode);
                },
                _ => {
                    return Err(eyre!(
                        "Should have returned an ActionErrorKind::PathModeMismatch error"
                    ))
                },
            },
            _ => {
                return Err(eyre!(
                    "Should have returned an ActionErrorKind::PathModeMismatch error"
                ))
            },
        }

        assert!(test_file.exists(), "File should have not been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn recognizes_correct_mode() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let test_file = temp_dir.path().join("recognizes_correct_mode");
        let initial_mode = 0o777;

        write(test_file.as_path(), "Some content").await?;
        tokio::fs::set_permissions(test_file.as_path(), PermissionsExt::from_mode(initial_mode))
            .await?;

        let mut action = CreateFile::plan(
            test_file.clone(),
            None,
            None,
            Some(initial_mode),
            "Some content".into(),
            false,
        )
        .await?;

        action.try_execute().await?;

        action.try_revert().await?;

        assert!(!test_file.exists(), "File should have been deleted");

        Ok(())
    }

    #[tokio::test]
    async fn errors_on_dir() -> eyre::Result<()> {
        let temp_dir = tempfile::tempdir()?;

        match CreateFile::plan(
            temp_dir.path(),
            None,
            None,
            None,
            "Some different content".into(),
            false,
        )
        .await
        {
            Err(err) => match err.kind() {
                ActionErrorKind::PathWasNotFile(path) => assert_eq!(path, temp_dir.path()),
                _ => {
                    return Err(eyre!(
                        "Should have returned an ActionErrorKind::PathWasNotFile error"
                    ))
                },
            },
            _ => {
                return Err(eyre!(
                    "Should have returned an ActionErrorKind::PathWasNotFile error"
                ))
            },
        }

        Ok(())
    }
}
