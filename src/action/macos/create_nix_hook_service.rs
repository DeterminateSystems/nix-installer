use serde::{Deserialize, Serialize};
use tracing::{span, Span};

use std::{
    fs::{self, Metadata, Permissions},
    io::{ErrorKind, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{fs::OpenOptions, io::AsyncWriteExt, process::Command};

use crate::{
    action::{Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction},
    execute_command,
    plan::PHASE1_RECEIPT_LOCATION,
    util::OnMissing,
};

use super::DARWIN_LAUNCHD_DOMAIN;

/** Create a plist for a `launchctl` service to re-add Nix to the zshrc after upgrades.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_nix_hook_service")]
pub struct CreateNixHookService {
    path: PathBuf,
    service_label: String,
    needs_bootout: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    named: Option<NamedHook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    legacy_upgrade: Option<LegacyUpgrade>,
}

const SERVICE_LABEL: &str = "systems.determinate.nix-installer.nix-hook";
const HELPER_PATH: &str = "/Library/PrivilegedHelperTools/nix-installer-repair";
const HELPER_SCRIPT: &[u8] =
    b"#!/bin/sh\n/bin/wait4path /nix/nix-installer && exec /nix/nix-installer repair\n";
const SERVICE_NOT_FOUND: i32 = 113;

type HookFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
struct LegacyUpgrade {
    plist: Option<FileIdentity>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct NamedHook {
    helper: PathBuf,
    token: String,
    directory: Option<FileIdentity>,
    executable: Option<FileIdentity>,
    plist: Option<FileIdentity>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
struct FileIdentity {
    inode: u64,
    created: std::time::SystemTime,
}

impl FileIdentity {
    fn read(metadata: &Metadata) -> std::io::Result<Self> {
        Ok(Self {
            inode: metadata.ino(),
            // APFS device identifiers can change after a reboot.
            created: metadata.created()?,
        })
    }
}

impl CreateNixHookService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let mut this = Self {
            path: format!("/Library/LaunchDaemons/{SERVICE_LABEL}.plist").into(),
            service_label: SERVICE_LABEL.into(),
            needs_bootout: false,
            legacy_upgrade: None,
            named: Some(NamedHook {
                helper: HELPER_PATH.into(),
                token: format!("nix-installer-repair-{:032x}", rand::random::<u128>()),
                directory: None,
                executable: None,
                plist: None,
            }),
        };
        if this.has_existing_hook().map_err(Self::error)? {
            return Ok(StatefulAction::skipped(this));
        }
        // The macOS package removes the old plist through split-receipt before
        // requesting a new plan. Recreate a legacy hook during that upgrade.
        if upgrading_legacy_hook(Path::new(PHASE1_RECEIPT_LOCATION)).map_err(Self::error)? {
            this.named = None;
            this.legacy_upgrade = Some(LegacyUpgrade::default());
        } else if this.loaded_job().await.map_err(Self::error)?.is_some() {
            return Err(Self::error(CreateNixHookServiceError::LoadedJob(
                this.service_label.clone(),
            )));
        }
        Ok(StatefulAction::uncompleted(this))
    }

    fn has_existing_hook(&self) -> Result<bool, ActionErrorKind> {
        if path_metadata(&self.path)?.is_none() {
            return Ok(false);
        }
        let file = open_safe(&self.path, false)?;
        require_no_acl(&file, &self.path)?;
        let mut discovered: LaunchctlHookPlist = plist::from_reader(file)
            .map_err(|_| CreateNixHookServiceError::UnrecognizedPlist(self.path.clone()))?;
        let expected = generate_plist(&self.service_label);
        if let [helper, token] = discovered.program_arguments.as_slice() {
            if self
                .named
                .as_ref()
                .is_some_and(|named| named.helper.as_os_str() == helper.as_str())
                && token
                    .strip_prefix("nix-installer-repair-")
                    .is_some_and(|suffix| {
                        suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
                    })
            {
                let helper = Path::new(helper);
                let directory = helper.parent().expect("helper directory");
                if path_metadata(directory)?.is_none() {
                    return Err(CreateNixHookServiceError::InvalidHelper(helper.into()).into());
                }
                let parent = open_safe(directory, true)?;
                if !open_helper(&parent, helper)?
                    .map(|file| matches_file(&file, helper, None, HELPER_SCRIPT, 0o700))
                    .transpose()?
                    .unwrap_or(false)
                {
                    return Err(CreateNixHookServiceError::InvalidHelper(helper.into()).into());
                }
                discovered
                    .program_arguments
                    .clone_from(&expected.program_arguments);
            }
        }
        if discovered != expected {
            return Err(CreateNixHookServiceError::UnrecognizedPlist(self.path.clone()).into());
        }
        Ok(true)
    }

    fn launchctl(&self, operation: &str) -> Command {
        let mut command = Command::new("/bin/launchctl");
        command
            .process_group(0)
            .arg(operation)
            .arg(format!("{DARWIN_LAUNCHD_DOMAIN}/{}", self.service_label))
            .stdin(Stdio::null());
        command
    }

    async fn loaded_job(&self) -> Result<Option<String>, ActionErrorKind> {
        let mut command = self.launchctl("print");
        let output = command
            .output()
            .await
            .map_err(|err| ActionErrorKind::command(&command, err))?;
        if output.status.success() {
            Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
        } else if output.status.code() == Some(SERVICE_NOT_FOUND) {
            Ok(None)
        } else {
            Err(ActionErrorKind::command_output(&command, output))
        }
    }

    fn named_plist(&self, named: &NamedHook) -> Result<Vec<u8>, ActionErrorKind> {
        let mut plist = generate_plist(&self.service_label);
        plist.program_arguments = vec![named.helper.display().to_string(), named.token.clone()];
        let mut bytes = Vec::new();
        plist::to_writer_xml(&mut bytes, &plist)?;
        Ok(bytes)
    }

    async fn revert_named(&mut self) -> Result<(), ActionErrorKind> {
        self.revert_named_with(
            |action| Box::pin(action.loaded_job()),
            |action| {
                Box::pin(async move {
                    execute_command(&mut action.launchctl("bootout")).await?;
                    Ok(())
                })
            },
        )
        .await
    }

    async fn revert_named_with(
        &mut self,
        inspect: impl for<'a> Fn(&'a Self) -> HookFuture<'a, Result<Option<String>, ActionErrorKind>>
            + Send,
        unload: impl for<'a> FnOnce(&'a Self) -> HookFuture<'a, Result<(), ActionErrorKind>> + Send,
    ) -> Result<(), ActionErrorKind> {
        let named = self.named.as_ref().expect("named hook action");
        let bytes = self.named_plist(named)?;
        let plist_is_ours = || matches_publication(&self.path, &named.plist, &bytes, 0o644);
        let cleanup_blocked = || CreateNixHookServiceError::CleanupBlocked {
            label: self.service_label.clone(),
            plist: self.path.clone(),
            helper: named.helper.clone(),
        };
        if plist_is_ours()? || (named.plist.is_some() && path_metadata(&self.path)?.is_none()) {
            if let Some(job) = inspect(self).await? {
                // launchctl print is diagnostic output. An unrecognized job
                // is retained along with both files.
                if !named.owns_job(&job, &self.path) {
                    return Err(cleanup_blocked().into());
                }
                unload(self).await?;
            }
            if inspect(self).await?.is_some() {
                return Err(cleanup_blocked().into());
            }
            if plist_is_ours()? {
                remove_published_file(&self.path)?;
            }
        }

        // Retain the helper for any remaining plist or loaded same-label job.
        if path_metadata(&self.path)?.is_none() {
            if inspect(self).await?.is_some() {
                if named.executable.is_some() {
                    return Err(cleanup_blocked().into());
                }
                return Ok(());
            }
            if path_metadata(&self.path)?.is_some() {
                return Ok(());
            }
            self.named
                .as_mut()
                .expect("named hook action")
                .remove_helper()?;
        }
        Ok(())
    }
}

impl NamedHook {
    fn owns_job(&self, job: &str, plist: &Path) -> bool {
        let field = |prefix| job.lines().find_map(|line| line.strip_prefix(prefix));
        if field("\tpath = ").map(Path::new) != Some(plist)
            || field("\tprogram = ").map(Path::new) != Some(&self.helper)
            || field("\tstdin path = ").is_some()
        {
            return false;
        }
        let mut arguments = job
            .lines()
            .skip_while(|line| *line != "\targuments = {")
            .skip(1);
        arguments
            .next()
            .and_then(|line| line.strip_prefix("\t\t"))
            .map(Path::new)
            == Some(&self.helper)
            && arguments.next().and_then(|line| line.strip_prefix("\t\t")) == Some(&self.token)
            && arguments.next() == Some("\t}")
    }

    fn publish(&mut self, plist: &Path, bytes: &[u8]) -> Result<(), ActionErrorKind> {
        // Refuse collisions before creating the helper and recheck atomically
        // when publishing the completed plist.
        if path_metadata(plist)?.is_some() {
            return Err(CreateNixHookServiceError::ExistingPath(plist.into()).into());
        }
        let directory = self.helper.parent().expect("helper directory");
        let create_directory = path_metadata(directory)?.is_none();
        if create_directory {
            fs::DirBuilder::new()
                .mode(0o755)
                .create(directory)
                .map_err(|err| ActionErrorKind::CreateDirectory(directory.into(), err))?;
        }
        let opened = open_path(directory, true)?;
        let metadata = opened
            .metadata()
            .map_err(|err| ActionErrorKind::GettingMetadata(directory.into(), err))?;
        if create_directory {
            self.directory = Some(file_identity(directory, &metadata)?);
        }
        if let Err(error) = validate_open_path(directory, &opened, &metadata, true) {
            if create_directory {
                if let Err(cleanup_error) = self.remove_directory() {
                    tracing::warn!(
                        ?cleanup_error,
                        "Could not remove the newly created hook directory"
                    );
                }
            }
            return Err(error);
        }
        if !matches_publication(&self.helper, &self.executable, HELPER_SCRIPT, 0o700)? {
            self.executable = None;
            if path_metadata(&self.helper)?.is_some() {
                return Err(CreateNixHookServiceError::HelperCollision(self.helper.clone()).into());
            }
            self.executable = Some(publish_file(&self.helper, HELPER_SCRIPT, 0o700)?);
        }
        self.plist = Some(publish_file(plist, bytes, 0o644)?);
        Ok(())
    }

    fn remove_helper(&mut self) -> Result<(), ActionErrorKind> {
        let directory = self.helper.parent().expect("helper directory");
        if !path_metadata(directory)?.is_some_and(|metadata| metadata.is_dir()) {
            return Ok(());
        }
        if let Some(recorded) = &self.executable {
            let parent = open_safe(directory, true)?;
            if let Some(file) = open_helper(&parent, &self.helper)? {
                if matches_file(&file, &self.helper, Some(recorded), HELPER_SCRIPT, 0o700)? {
                    use nix::unistd::{unlinkat, UnlinkatFlags};
                    let name = self.helper.file_name().expect("helper name");
                    match unlinkat(Some(parent.as_raw_fd()), name, UnlinkatFlags::NoRemoveDir) {
                        Ok(()) | Err(nix::errno::Errno::ENOENT) => {},
                        Err(err) => {
                            return Err(ActionErrorKind::Remove(self.helper.clone(), err.into()))
                        },
                    }
                }
            }
        }
        self.executable = None;
        self.remove_directory()
    }

    fn remove_directory(&mut self) -> Result<(), ActionErrorKind> {
        let directory = self.helper.parent().expect("helper directory");
        if let (Some(recorded), Some(metadata)) = (&self.directory, path_metadata(directory)?) {
            if metadata.is_dir()
                && metadata.uid() == nix::unistd::geteuid().as_raw()
                && metadata.mode() & 0o022 == 0
                && !is_unlink_protected(&metadata)
                && file_identity(directory, &metadata)? == *recorded
            {
                match fs::remove_dir(directory) {
                    Ok(()) => {},
                    Err(err)
                        if matches!(
                            err.kind(),
                            ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty
                        ) => {},
                    Err(err) => return Err(ActionErrorKind::Remove(directory.into(), err)),
                }
            }
        }
        self.directory = None;
        Ok(())
    }
}

fn upgrading_legacy_hook(path: &Path) -> Result<bool, ActionErrorKind> {
    #[derive(Deserialize)]
    struct Receipt {
        actions: Vec<serde_json::Value>,
    }

    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(ActionErrorKind::Read(path.into(), err)),
    };
    let receipt: Receipt =
        serde_json::from_slice(&bytes).map_err(|err| ActionErrorKind::Custom(Box::new(err)))?;
    Ok(receipt.actions.iter().any(|entry| {
        let action = &entry["action"];
        action["action_name"]
            .as_str()
            .or_else(|| action["action"].as_str())
            == Some("create_nix_hook_service")
            && action["named"].is_null()
    }))
}

fn path_metadata(path: &Path) -> Result<Option<Metadata>, ActionErrorKind> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(ActionErrorKind::GettingMetadata(path.into(), err)),
    }
}

fn file_identity(path: &Path, metadata: &Metadata) -> Result<FileIdentity, ActionErrorKind> {
    FileIdentity::read(metadata).map_err(|err| ActionErrorKind::GettingMetadata(path.into(), err))
}

fn open_path(path: &Path, directory: bool) -> Result<fs::File, ActionErrorKind> {
    let flags = nix::libc::O_NOFOLLOW
        | nix::libc::O_NONBLOCK
        | if directory { nix::libc::O_DIRECTORY } else { 0 };
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(path)
        .map_err(|err| ActionErrorKind::Open(path.into(), err))
}

fn open_safe(path: &Path, directory: bool) -> Result<fs::File, ActionErrorKind> {
    let file = open_path(path, directory)?;
    let metadata = file
        .metadata()
        .map_err(|err| ActionErrorKind::GettingMetadata(path.into(), err))?;
    validate_open_path(path, &file, &metadata, directory)?;
    Ok(file)
}

fn validate_open_path(
    path: &Path,
    file: &fs::File,
    metadata: &Metadata,
    directory: bool,
) -> Result<(), ActionErrorKind> {
    if (if directory {
        !metadata.is_dir()
    } else {
        !metadata.is_file()
    }) || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(CreateNixHookServiceError::UnsafePath(path.into()).into());
    }
    if directory {
        require_no_acl(file, path)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn has_acl(file: &fs::File, path: &Path) -> Result<bool, ActionErrorKind> {
    use nix::libc::{c_int, c_void, ENOENT};
    extern "C" {
        fn acl_get_fd(fd: c_int) -> *mut c_void;
        fn acl_free(acl: *mut c_void) -> c_int;
    }
    // SAFETY: file owns a valid descriptor for the duration of the call.
    let acl = unsafe { acl_get_fd(file.as_raw_fd()) };
    if acl.is_null() {
        let err = std::io::Error::last_os_error();
        return if err.raw_os_error() == Some(ENOENT) {
            Ok(false)
        } else {
            Err(ActionErrorKind::GettingMetadata(path.into(), err))
        };
    }
    // SAFETY: acl_get_fd returned an allocated ACL owned by this function.
    unsafe { acl_free(acl) };
    Ok(true)
}

#[cfg(not(target_os = "macos"))]
fn has_acl(_: &fs::File, _: &Path) -> Result<bool, ActionErrorKind> {
    Ok(false)
}

fn require_no_acl(file: &fs::File, path: &Path) -> Result<(), ActionErrorKind> {
    if has_acl(file, path)? {
        return Err(CreateNixHookServiceError::UnsafePath(path.into()).into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn is_unlink_protected(metadata: &Metadata) -> bool {
    use nix::libc::{SF_APPEND, SF_IMMUTABLE, UF_APPEND, UF_IMMUTABLE};
    use std::os::macos::fs::MetadataExt as _;
    metadata.st_flags() & (SF_APPEND | SF_IMMUTABLE | UF_APPEND | UF_IMMUTABLE) != 0
}

#[cfg(not(target_os = "macos"))]
fn is_unlink_protected(_: &Metadata) -> bool {
    false
}

fn publish_file(path: &Path, bytes: &[u8], mode: u32) -> Result<FileIdentity, ActionErrorKind> {
    let parent = path.parent().expect("publication directory");
    let mut staged = tempfile::NamedTempFile::new_in(parent)
        .map_err(|err| ActionErrorKind::Open(path.into(), err))?;
    staged
        .write_all(bytes)
        .map_err(|err| ActionErrorKind::Write(path.into(), err))?;
    let file = staged.as_file();
    if nix::unistd::geteuid().is_root() {
        nix::unistd::fchown(file.as_raw_fd(), None, Some(nix::unistd::Gid::from_raw(0)))
            .map_err(|err| ActionErrorKind::Chown(path.into(), err))?;
    }
    file.set_permissions(Permissions::from_mode(mode))
        .map_err(|err| ActionErrorKind::SetPermissions(mode, path.into(), err))?;
    require_no_acl(file, path)?;
    let metadata = file
        .metadata()
        .map_err(|err| ActionErrorKind::GettingMetadata(path.into(), err))?;
    let identity = file_identity(path, &metadata)?;
    let temporary_path = staged.path().to_owned();
    staged
        .persist_noclobber(path)
        .map_err(|err| ActionErrorKind::Rename(temporary_path, path.into(), err.error))?;
    Ok(identity)
}

fn open_helper(parent: &fs::File, path: &Path) -> Result<Option<fs::File>, ActionErrorKind> {
    use nix::{
        errno::Errno,
        fcntl::{openat, OFlag},
        sys::stat::Mode,
    };

    let name = path.file_name().expect("helper name");
    match openat(
        Some(parent.as_raw_fd()),
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    ) {
        // SAFETY: openat returned a new descriptor and File takes ownership.
        Ok(fd) => Ok(Some(unsafe { fs::File::from_raw_fd(fd) })),
        Err(Errno::ENOENT | Errno::ELOOP) => Ok(None),
        Err(err) => Err(ActionErrorKind::Open(path.into(), err.into())),
    }
}

fn matches_publication(
    path: &Path,
    recorded: &Option<FileIdentity>,
    bytes: &[u8],
    mode: u32,
) -> Result<bool, ActionErrorKind> {
    let (Some(recorded), Some(metadata)) = (recorded, path_metadata(path)?) else {
        return Ok(false);
    };
    if !metadata.is_file()
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.mode() & 0o7777 != mode
        || file_identity(path, &metadata)? != *recorded
    {
        return Ok(false);
    }
    matches_file(&open_safe(path, false)?, path, Some(recorded), bytes, mode)
}

fn matches_file(
    file: &fs::File,
    path: &Path,
    recorded: Option<&FileIdentity>,
    bytes: &[u8],
    mode: u32,
) -> Result<bool, ActionErrorKind> {
    use std::io::Read;

    let opened = file
        .metadata()
        .map_err(|err| ActionErrorKind::GettingMetadata(path.into(), err))?;
    if !opened.is_file()
        || opened.uid() != nix::unistd::geteuid().as_raw()
        || opened.mode() & 0o7777 != mode
        || (nix::unistd::geteuid().is_root() && opened.gid() != 0)
        || is_unlink_protected(&opened)
        || has_acl(file, path)?
    {
        return Ok(false);
    }
    if let Some(recorded) = recorded {
        if file_identity(path, &opened)? != *recorded {
            return Ok(false);
        }
    }
    let mut current = Vec::new();
    file.take(bytes.len() as u64 + 1)
        .read_to_end(&mut current)
        .map_err(|err| ActionErrorKind::Read(path.into(), err))?;
    Ok(current == bytes)
}

fn remove_published_file(path: &Path) -> Result<(), ActionErrorKind> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(ActionErrorKind::Remove(path.into(), err)),
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_nix_hook_service")]
impl Action for CreateNixHookService {
    fn action_tag() -> ActionTag {
        ActionTag("create_nix_hook_service")
    }
    fn tracing_synopsis(&self) -> String {
        if self.named.is_some() {
            return "Install the named macOS Nix repair hook".into();
        }
        format!(
            "{maybe_unload} a `launchctl` plist to put Nix into your PATH",
            maybe_unload = if self.needs_bootout {
                "Unload, then recreate"
            } else {
                "Create"
            }
        )
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_nix_hook_service",
            path = tracing::field::display(self.path.display()),
            buf = tracing::field::Empty,
        );

        span
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        if let Some(named) = &self.named {
            if self.loaded_job().await.map_err(Self::error)?.is_some() {
                return Err(Self::error(CreateNixHookServiceError::LoadedJob(
                    self.service_label.clone(),
                )));
            }
            let bytes = self.named_plist(named).map_err(Self::error)?;
            return self
                .named
                .as_mut()
                .expect("named hook action")
                .publish(&self.path, &bytes)
                .map_err(Self::error);
        }
        if let Some(upgrade) = &mut self.legacy_upgrade {
            let mut bytes = Vec::new();
            plist::to_writer_xml(&mut bytes, &generate_plist(&self.service_label))
                .map_err(Self::error)?;
            if !matches_publication(&self.path, &upgrade.plist, &bytes, 0o644)
                .map_err(Self::error)?
            {
                upgrade.plist = Some(publish_file(&self.path, &bytes, 0o644).map_err(Self::error)?);
            }
            return Ok(());
        }
        let Self {
            path,
            service_label,
            needs_bootout,
            named: _,
            legacy_upgrade: _,
        } = self;

        if *needs_bootout {
            crate::action::macos::retry_bootout(DARWIN_LAUNCHD_DOMAIN, service_label)
                .await
                .map_err(Self::error)?;
        }

        let generated_plist = generate_plist(service_label);

        let mut options = OpenOptions::new();
        options.create(true).write(true).read(true);

        let mut file = options
            .open(&path)
            .await
            .map_err(|e| Self::error(ActionErrorKind::Open(path.to_owned(), e)))?;

        let mut buf = Vec::new();
        plist::to_writer_xml(&mut buf, &generated_plist).map_err(Self::error)?;
        file.write_all(&buf)
            .await
            .map_err(|e| Self::error(ActionErrorKind::Write(path.to_owned(), e)))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        if self.named.is_some() {
            return vec![ActionDescription::new(
                "Remove the installer-owned macOS Nix repair hook".into(),
                vec![],
            )];
        }
        vec![ActionDescription::new(
            format!("Delete file `{}`", self.path.display()),
            vec![format!("Delete file `{}`", self.path.display())],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        if self.named.is_some() {
            return self.revert_named().await.map_err(Self::error);
        }
        if let Some(upgrade) = &mut self.legacy_upgrade {
            let mut bytes = Vec::new();
            plist::to_writer_xml(&mut bytes, &generate_plist(&self.service_label))
                .map_err(Self::error)?;
            if matches_publication(&self.path, &upgrade.plist, &bytes, 0o644)
                .map_err(Self::error)?
            {
                remove_published_file(&self.path).map_err(Self::error)?;
            }
            upgrade.plist = None;
            return Ok(());
        }
        crate::util::remove_file(&self.path, OnMissing::Ignore)
            .await
            .map_err(|e| Self::error(ActionErrorKind::Remove(self.path.to_owned(), e)))?;

        Ok(())
    }
}

fn generate_plist(service_label: &str) -> LaunchctlHookPlist {
    LaunchctlHookPlist {
        keep_alive: KeepAliveOpts {
            successful_exit: false,
        },
        label: service_label.into(),
        program_arguments: vec![
            "/bin/sh".into(),
            "-c".into(),
            "/bin/wait4path /nix/nix-installer && /nix/nix-installer repair".into(),
        ],
        standard_error_path: "/nix/.nix-installer-hook.err.log".into(),
        standard_out_path: "/nix/.nix-installer-hook.out.log".into(),
    }
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct LaunchctlHookPlist {
    label: String,
    program_arguments: Vec<String>,
    keep_alive: KeepAliveOpts,
    standard_error_path: String,
    standard_out_path: String,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct KeepAliveOpts {
    successful_exit: bool,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum CreateNixHookServiceError {
    #[error("Cannot install the Nix repair hook because `{0}` appeared after planning")]
    ExistingPath(PathBuf),
    #[error("The Nix repair hook path `{0}` has unexpected ownership or permissions")]
    UnsafePath(PathBuf),
    #[error("The repair-hook helper at `{0}` is missing or invalid. Review the helper and its plist before retrying installation.")]
    InvalidHelper(PathBuf),
    #[error("A repair-hook helper already exists at `{0}`. If a previous installation stopped midway, inspect this file and any install receipt before moving it aside and retrying.")]
    HelperCollision(PathBuf),
    #[error("The existing repair-hook plist at `{0}` is unrecognized. Review it before retrying installation.")]
    UnrecognizedPlist(PathBuf),
    #[error("A job named `{0}` is already loaded. If it remains from a previous uninstall, restart macOS and retry. If it is a custom job, review its configuration before retrying.")]
    LoadedJob(String),
    #[error("The loaded job `{label}` blocks repair-hook cleanup. Check the job and any remaining files at `{plist}` and `{helper}` before retrying with the install receipt, if available.")]
    CleanupBlocked {
        label: String,
        plist: PathBuf,
        helper: PathBuf,
    },
    #[error(
        "`{path}` exists and contains content different than expected. Consider removing the file."
    )]
    DifferentPlist {
        expected: LaunchctlHookPlist,
        discovered: LaunchctlHookPlist,
        path: PathBuf,
    },
}

impl From<CreateNixHookServiceError> for ActionErrorKind {
    fn from(val: CreateNixHookServiceError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(directory: &Path) -> CreateNixHookService {
        CreateNixHookService {
            path: directory.join("hook.plist"),
            service_label: SERVICE_LABEL.into(),
            needs_bootout: false,
            legacy_upgrade: None,
            named: Some(NamedHook {
                helper: directory.join("helpers/nix-installer-repair"),
                token: "nix-installer-repair-test".into(),
                directory: None,
                executable: None,
                plist: None,
            }),
        }
    }

    fn publish(action: &mut CreateNixHookService) -> Result<Vec<u8>, ActionErrorKind> {
        let bytes = action.named_plist(action.named.as_ref().unwrap())?;
        action
            .named
            .as_mut()
            .unwrap()
            .publish(&action.path, &bytes)?;
        Ok(bytes)
    }

    #[test]
    fn publishes_named_hook_and_removes_its_helper() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        let bytes = publish(&mut action).unwrap();
        let named = action.named.as_mut().unwrap();
        let plist: LaunchctlHookPlist = plist::from_bytes(&bytes).unwrap();
        assert_eq!(
            plist.program_arguments,
            [named.helper.display().to_string(), named.token.clone()]
        );
        assert!(!plist.keep_alive.successful_exit);
        assert!(matches_publication(&action.path, &named.plist, &bytes, 0o644).unwrap());
        assert!(
            matches_publication(&named.helper, &named.executable, HELPER_SCRIPT, 0o700).unwrap()
        );
        remove_published_file(&action.path).unwrap();
        named.remove_helper().unwrap();
        assert!(!named.helper.parent().unwrap().exists());
    }

    #[test]
    fn leaves_existing_plist_and_helper_alone() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        fs::write(&action.path, "existing plist").unwrap();
        assert!(publish(&mut action).is_err());
        let helper = action.named.as_ref().unwrap().helper.clone();
        assert!(!helper.parent().unwrap().exists());
        assert_eq!(fs::read(&action.path).unwrap(), b"existing plist");
        fs::remove_file(&action.path).unwrap();

        let parent = helper.parent().unwrap();
        fs::create_dir(parent).unwrap();
        fs::set_permissions(parent, Permissions::from_mode(0o755)).unwrap();
        fs::write(&helper, "existing helper").unwrap();
        assert!(publish(&mut action).is_err());
        action.named.as_mut().unwrap().remove_helper().unwrap();
        assert_eq!(fs::read(&helper).unwrap(), b"existing helper");
        assert!(!action.path.exists());
    }

    #[test]
    fn leaves_a_matching_replacement_helper_alone() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        publish(&mut action).unwrap();
        let named = action.named.as_mut().unwrap();
        let replacement = directory.path().join("replacement");
        publish_file(&replacement, HELPER_SCRIPT, 0o700).unwrap();
        fs::rename(replacement, &named.helper).unwrap();
        named.remove_helper().unwrap();
        assert_eq!(fs::read(&named.helper).unwrap(), HELPER_SCRIPT);
    }

    #[test]
    fn recognizes_only_the_expected_loaded_job() {
        let directory = tempfile::tempdir().unwrap();
        let action = fixture(directory.path());
        let named = action.named.as_ref().unwrap();
        let job = format!(
            "\tpath = {}\n\tprogram = {}\n\targuments = {{\n\t\t{}\n\t\t{}\n\t}}\n",
            action.path.display(),
            named.helper.display(),
            named.helper.display(),
            named.token,
        );
        assert!(named.owns_job(&job, &action.path));
        assert!(!named.owns_job(&job.replace(&named.token, "other"), &action.path));
        assert!(!named.owns_job(&format!("{job}\tstdin path = /dev/null\n"), &action.path));
    }

    #[test]
    fn split_receipt_keeps_a_legacy_hook_on_the_upgrade_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("uninstall-phase1.json");
        assert!(!upgrading_legacy_hook(&path).unwrap());

        let action = fixture(directory.path());
        let mut hook = serde_json::to_value(action).unwrap();
        let write_receipt = |hook: &serde_json::Value| {
            fs::write(
                &path,
                serde_json::to_vec(&serde_json::json!({"actions": [{"action": hook}]})).unwrap(),
            )
            .unwrap();
        };
        write_receipt(&hook);
        assert!(!upgrading_legacy_hook(&path).unwrap());

        hook.as_object_mut().unwrap().remove("named");
        write_receipt(&hook);
        assert!(upgrading_legacy_hook(&path).unwrap());
        let fields = hook.as_object_mut().unwrap();
        let tag = fields.remove("action_name").unwrap();
        fields.insert("action".into(), tag);
        write_receipt(&hook);
        assert!(upgrading_legacy_hook(&path).unwrap());
    }

    #[tokio::test]
    async fn legacy_upgrade_preserves_plist_that_appeared_after_planning() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        action.named = None;
        action.legacy_upgrade = Some(LegacyUpgrade::default());
        fs::write(&action.path, "another plist").unwrap();
        assert!(action.execute().await.is_err());
        action.revert().await.unwrap();
        assert_eq!(fs::read(&action.path).unwrap(), b"another plist");

        fs::remove_file(&action.path).unwrap();
        action.execute().await.unwrap();
        assert!(action.has_existing_hook().unwrap());
        action.revert().await.unwrap();
        assert!(!action.path.exists());
    }

    #[test]
    fn existing_hook_validation_preserves_collisions() {
        let directory = tempfile::tempdir().unwrap();
        let action = fixture(directory.path());
        fs::write(&action.path, "invalid plist").unwrap();
        fs::set_permissions(&action.path, Permissions::from_mode(0o644)).unwrap();
        assert!(action.has_existing_hook().is_err());
        assert_eq!(fs::read(&action.path).unwrap(), b"invalid plist");
        let mut named = action.named.clone().unwrap();
        named.token = format!("nix-installer-repair-{:032x}", 1);
        let bytes = action.named_plist(&named).unwrap();
        fs::write(&action.path, bytes).unwrap();
        let parent = named.helper.parent().unwrap();
        fs::create_dir(parent).unwrap();
        fs::set_permissions(parent, Permissions::from_mode(0o755)).unwrap();
        assert!(action.has_existing_hook().is_err());
        publish_file(&named.helper, HELPER_SCRIPT, 0o700).unwrap();
        assert!(action.has_existing_hook().unwrap());

        let mut invalid = named.clone();
        invalid.helper.push(".");
        fs::write(&action.path, action.named_plist(&invalid).unwrap()).unwrap();
        assert!(action.has_existing_hook().is_err());

        fs::write(&action.path, action.named_plist(&named).unwrap()).unwrap();
        fs::set_permissions(&named.helper, Permissions::from_mode(0o600)).unwrap();
        assert!(action.has_existing_hook().is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn existing_plist_with_an_acl_is_rejected_before_skipping() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        action.named.as_mut().unwrap().token = format!("nix-installer-repair-{:032x}", 1);
        let named_bytes = publish(&mut action).unwrap();
        assert!(action.has_existing_hook().unwrap());

        let legacy = directory.path().join("legacy.plist");
        let mut legacy_bytes = Vec::new();
        plist::to_writer_xml(&mut legacy_bytes, &generate_plist(SERVICE_LABEL)).unwrap();
        publish_file(&legacy, &legacy_bytes, 0o644).unwrap();

        for (path, bytes) in [(action.path.clone(), named_bytes), (legacy, legacy_bytes)] {
            action.path = path;
            assert!(action.has_existing_hook().unwrap());
            assert!(std::process::Command::new("/bin/chmod")
                .args(["+a", "everyone allow write"])
                .arg(&action.path)
                .status()
                .unwrap()
                .success());
            assert_eq!(fs::metadata(&action.path).unwrap().mode() & 0o777, 0o644);
            let error = action.has_existing_hook().unwrap_err();
            assert!(matches!(
                error,
                ActionErrorKind::Custom(ref err)
                    if matches!(
                        err.downcast_ref::<CreateNixHookServiceError>(),
                        Some(CreateNixHookServiceError::UnsafePath(path)) if path == &action.path
                    )
            ));
            assert_eq!(fs::read(&action.path).unwrap(), bytes);
        }
    }

    #[test]
    fn rollback_preserves_a_helper_reached_through_a_relocated_directory() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        publish(&mut action).unwrap();
        let named = action.named.as_mut().unwrap();
        let original = named.helper.parent().unwrap().to_owned();
        let relocated = directory.path().join("relocated");
        fs::rename(&original, &relocated).unwrap();
        std::os::unix::fs::symlink(&relocated, &original).unwrap();

        named.remove_helper().unwrap();
        assert_eq!(
            fs::read(relocated.join(named.helper.file_name().unwrap())).unwrap(),
            HELPER_SCRIPT
        );
        assert!(fs::symlink_metadata(original).unwrap().is_symlink());
    }

    #[tokio::test]
    async fn rollback_retains_helper_when_plist_reappears_during_job_lookup() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        let bytes = publish(&mut action).unwrap();
        fs::remove_file(&action.path).unwrap();
        let helper = action.named.as_ref().unwrap().helper.clone();

        use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
        let lookups = AtomicUsize::new(0);
        action
            .revert_named_with(
                |action| {
                    if lookups.fetch_add(1, Relaxed) == 2 {
                        fs::write(&action.path, &bytes).unwrap();
                    }
                    Box::pin(std::future::ready(Ok(None)))
                },
                |_| Box::pin(async { panic!("no job is loaded") }),
            )
            .await
            .unwrap();
        assert_eq!(lookups.load(Relaxed), 3);
        assert_eq!(fs::read(&action.path).unwrap(), bytes);
        assert_eq!(fs::read(helper).unwrap(), HELPER_SCRIPT);
    }

    #[tokio::test]
    async fn rollback_reports_blocked_cleanup_and_preserves_both_files() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        let bytes = publish(&mut action).unwrap();
        let helper = action.named.as_ref().unwrap().helper.clone();

        let error = action
            .revert_named_with(
                |_| Box::pin(async { Ok(Some("foreign job".into())) }),
                |_| Box::pin(async { panic!("foreign job must be retained") }),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("blocks repair-hook cleanup"));
        assert_eq!(fs::read(&action.path).unwrap(), bytes);
        assert_eq!(fs::read(helper).unwrap(), HELPER_SCRIPT);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn inherited_acl_failure_removes_only_a_new_empty_helper_directory() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        let parent = action
            .named
            .as_ref()
            .unwrap()
            .helper
            .parent()
            .unwrap()
            .to_owned();
        assert!(std::process::Command::new("/bin/chmod")
            .args(["+a", "everyone allow add_file,directory_inherit"])
            .arg(directory.path())
            .status()
            .unwrap()
            .success());

        assert!(publish(&mut action)
            .unwrap_err()
            .to_string()
            .contains("unexpected ownership or permissions"));
        assert!(!parent.exists());
        assert!(!action.path.exists());

        fs::create_dir(&parent).unwrap();
        assert!(publish(&mut action).is_err());
        assert!(parent.is_dir());
        assert!(!action.path.exists());
    }

    #[tokio::test]
    async fn missing_plist_still_allows_owned_job_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let mut action = fixture(directory.path());
        publish(&mut action).unwrap();
        fs::remove_file(&action.path).unwrap();
        let named = action.named.as_ref().unwrap();
        let helper = named.helper.clone();
        let job = format!(
            "\tpath = {}\n\tprogram = {}\n\targuments = {{\n\t\t{}\n\t\t{}\n\t}}\n",
            action.path.display(),
            helper.display(),
            helper.display(),
            named.token
        );
        let error = action
            .revert_named_with(
                |_| Box::pin(async { Ok(Some("foreign job".into())) }),
                |_| Box::pin(async { panic!("foreign job must be retained") }),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("blocks repair-hook cleanup"));
        assert!(helper.exists());

        let unloaded = std::sync::atomic::AtomicBool::new(false);
        use std::sync::atomic::Ordering::Relaxed;
        action
            .revert_named_with(
                |_| {
                    Box::pin(std::future::ready(Ok(
                        (!unloaded.load(Relaxed)).then(|| job.clone())
                    )))
                },
                |_| {
                    unloaded.store(true, Relaxed);
                    Box::pin(std::future::ready(Ok(())))
                },
            )
            .await
            .unwrap();
        assert!(unloaded.load(Relaxed));
        assert!(!helper.exists());
    }

    #[tokio::test]
    async fn old_receipt_removes_its_legacy_plist() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let mut action: CreateNixHookService = serde_json::from_value(serde_json::json!({
            "action_name": "create_nix_hook_service",
            "path": path,
            "service_label": SERVICE_LABEL,
            "needs_bootout": false
        }))
        .unwrap();
        fs::write(&path, "legacy plist").unwrap();
        action.revert().await.unwrap();
        assert!(!path.exists());
    }
}
