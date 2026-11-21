use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{span, Span};

use std::{
    fs as std_fs,
    future::Future,
    io::ErrorKind,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::{fs, process::Command};

use crate::{
    action::{
        base::{CreateDirectory, CreateFile},
        Action, ActionDescription, ActionError, ActionErrorKind, ActionState, ActionTag,
        StatefulAction,
    },
    util::OnMissing,
};

use super::DARWIN_LAUNCHD_DOMAIN;

const LAUNCHER_DIRECTORY: &str = "/Library/PrivilegedHelperTools";
const LAUNCHER_PATH: &str = "/Library/PrivilegedHelperTools/nix-installer-repair";
const LAUNCHER_SCRIPT: &str =
    "#!/bin/sh\n/bin/wait4path /nix/nix-installer && exec /nix/nix-installer repair\n";
const SERVICE_LABEL: &str = "systems.determinate.nix-installer.nix-hook";
const LEGACY_PROGRAM: &str = "/bin/sh";
const LEGACY_COMMAND: &str = "/bin/wait4path /nix/nix-installer && /nix/nix-installer repair";
const HOOK_STDERR: &str = "/nix/.nix-installer-hook.err.log";
const HOOK_STDOUT: &str = "/nix/.nix-installer-hook.out.log";
const LAUNCHCTL_SERVICE_NOT_FOUND: i32 = 113;

/** Create a plist for a `launchctl` service to re-add Nix to the zshrc after upgrades.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_nix_hook_service")]
pub struct CreateNixHookService {
    path: PathBuf,
    service_label: String,
    needs_bootout: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    launcher: Option<HookLauncher>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct HookLauncher {
    #[serde(default)]
    owns_file: bool,
    directory: Option<StatefulAction<CreateDirectory>>,
    file: StatefulAction<CreateFile>,
    #[serde(default)]
    owns_plist: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    job_token: Option<String>,
}

#[derive(PartialEq, Eq)]
enum PlistState {
    Missing,
    Legacy,
    Named,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoadedJob {
    Absent,
    Owned,
    Legacy,
    Other,
}

impl HookLauncher {
    async fn file_is_ready(&self) -> Result<bool, ActionError> {
        let file = self.file.inner();
        match fs::symlink_metadata(&file.path).await {
            Ok(metadata) if metadata.is_file() => {
                Ok(file.replan().await?.state == ActionState::Completed)
            },
            Ok(_) => Err(CreateNixHookService::error(
                ActionErrorKind::PathWasNotFile(file.path.clone()),
            )),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(CreateNixHookService::error(
                ActionErrorKind::GettingMetadata(file.path.clone(), error),
            )),
        }
    }

    async fn install(&mut self) -> Result<(), ActionError> {
        let file = self.file.inner();
        let parent = file.path.parent().unwrap_or_else(|| Path::new("."));
        let (owner, _) = file.owner_ids()?;
        let owner = owner.map(|uid| uid.as_raw());
        if !launcher_directory_exists(parent, owner).map_err(CreateNixHookService::error)? {
            let directory = CreateDirectory::plan(
                parent,
                file.user.clone(),
                file.group.clone(),
                Some(0o755),
                false,
            )
            .await?;
            if directory.state == ActionState::Uncompleted {
                let directory = self.directory.insert(directory);
                directory.try_execute().await?;
            }
        }
        if !self.file_is_ready().await? {
            // The previous file action may describe a launcher that vanished
            // since this plan was saved. Publication is exclusive and atomic.
            self.file = self.file.inner().replan_atomically().await?;
            if self.file.state == ActionState::Uncompleted {
                self.file.try_execute().await?;
                self.owns_file = true;
            }
        }
        Ok(())
    }
}

impl CreateNixHookService {
    async fn plan_launcher() -> Result<HookLauncher, ActionError> {
        let directory = Path::new(LAUNCHER_DIRECTORY);
        let directory_action =
            if launcher_directory_exists(directory, Some(0)).map_err(Self::error)? {
                None
            } else {
                let action = CreateDirectory::plan(
                    directory,
                    Some("root".into()),
                    Some("wheel".into()),
                    Some(0o755),
                    false,
                )
                .await?;
                (action.state == ActionState::Uncompleted).then_some(action)
            };

        match fs::symlink_metadata(LAUNCHER_PATH).await {
            Ok(metadata) if !metadata.is_file() => {
                return Err(Self::error(ActionErrorKind::PathWasNotFile(
                    LAUNCHER_PATH.into(),
                )));
            },
            Ok(_) => (),
            Err(error) if error.kind() == ErrorKind::NotFound => (),
            Err(error) => {
                return Err(Self::error(ActionErrorKind::GettingMetadata(
                    LAUNCHER_PATH.into(),
                    error,
                )));
            },
        }
        let file = CreateFile::plan(
            LAUNCHER_PATH,
            Some("root".into()),
            Some("wheel".into()),
            Some(0o700),
            LAUNCHER_SCRIPT.into(),
            false,
        )
        .await?;
        Ok(HookLauncher {
            owns_file: false,
            directory: directory_action,
            file,
            owns_plist: false,
            job_token: None,
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        Self::plan_at(
            format!("/Library/LaunchDaemons/{SERVICE_LABEL}.plist").into(),
            Self::plan_launcher().await?,
        )
    }

    fn plan_at(
        path: PathBuf,
        mut launcher: HookLauncher,
    ) -> Result<StatefulAction<Self>, ActionError> {
        launcher.job_token = Some(format!(
            "nix-installer-repair-{:032x}",
            rand::random::<u128>()
        ));
        let this = Self {
            path,
            service_label: SERVICE_LABEL.into(),
            needs_bootout: false,
            launcher: Some(launcher),
        };
        this.plist_state()?;
        // Applying a saved plan must recheck the launcher, even if it was
        // present when this plan was generated.
        Ok(StatefulAction::uncompleted(this))
    }

    fn plist_state(&self) -> Result<PlistState, ActionError> {
        let Some(discovered) = read_plist::<LaunchctlHookPlist>(&self.path).map_err(Self::error)?
        else {
            return Ok(PlistState::Missing);
        };
        let expected = generate_plist(&self.service_label);
        let mut named = discovered.clone();
        if named.program_arguments.len() == 2 && valid_job_token(&named.program_arguments[1]) {
            named.program_arguments.pop();
        }
        if named == expected {
            Ok(PlistState::Named)
        } else if discovered == legacy_plist(&self.service_label) {
            Ok(PlistState::Legacy)
        } else {
            Err(Self::error(CreateNixHookServiceError::DifferentPlist {
                expected,
                discovered,
                path: self.path.clone(),
            }))
        }
    }

    fn install_named_plist(&mut self) -> Result<(), ActionError> {
        let plist = installed_plist(
            &self.service_label,
            self.launcher
                .as_ref()
                .and_then(|launcher| launcher.job_token.as_deref()),
        );
        let temporary = stage_plist(&self.path, &plist).map_err(Self::error)?;
        let temporary_path = temporary.path().to_owned();

        // Recheck after preparing the launcher and replacement. Rename is
        // atomic, but this check and rename cannot exclude an external writer.
        let installed = match self.plist_state()? {
            PlistState::Missing => temporary.persist_noclobber(&self.path),
            PlistState::Legacy => temporary.persist(&self.path),
            PlistState::Named => return Ok(()),
        };
        installed.map_err(|error| {
            Self::error(ActionErrorKind::Rename(
                temporary_path,
                self.path.clone(),
                error.error,
            ))
        })?;
        if let Some(launcher) = &mut self.launcher {
            launcher.owns_plist = true;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_nix_hook_service")]
impl Action for CreateNixHookService {
    fn action_tag() -> ActionTag {
        ActionTag("create_nix_hook_service")
    }
    fn tracing_synopsis(&self) -> String {
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
        let mut description = Vec::new();
        if let Some(launcher) = &self.launcher {
            if let Some(directory) = &launcher.directory {
                description.extend(directory.describe_execute());
            }
            description.extend(launcher.file.describe_execute());
        }
        description.push(ActionDescription::new(self.tracing_synopsis(), vec![]));
        description
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        if self.launcher.is_some() {
            self.plist_state()?;
        }
        if let Some(launcher) = &mut self.launcher {
            launcher.install().await?;
            // launchd reads the replacement on its next load.
            self.install_named_plist()?;
        } else {
            // Preserve the behavior of a plan saved by an older installer.
            if self.needs_bootout {
                crate::action::macos::retry_bootout(DARWIN_LAUNCHD_DOMAIN, &self.service_label)
                    .await
                    .map_err(Self::error)?;
            }
            write_plist(&self.path, &legacy_plist(&self.service_label)).map_err(Self::error)?;
        }
        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let mut description = Vec::new();
        if self
            .launcher
            .as_ref()
            .is_none_or(|launcher| launcher.owns_plist)
        {
            description.push(ActionDescription::new(
                format!("Delete file `{}`", self.path.display()),
                vec![format!("Delete file `{}`", self.path.display())],
            ));
        }
        if let Some(launcher) = &self.launcher {
            if launcher.owns_file {
                description.extend(launcher.file.describe_revert());
            }
            if let Some(directory) = &launcher.directory {
                description.extend(directory.describe_revert());
            }
        }
        description
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let service = self.service_label.clone();
        let path = self.path.clone();
        let token = self
            .launcher
            .as_ref()
            .and_then(|launcher| launcher.job_token.clone());
        self.revert_with(
            || bootout_named_job(&service),
            || inspect_loaded_job(&service, &path, token.as_deref()),
        )
        .await
    }
}

impl CreateNixHookService {
    async fn revert_with<U, P>(
        &mut self,
        unload: impl FnOnce() -> U,
        inspect: impl Fn() -> P,
    ) -> Result<(), ActionError>
    where
        U: Future<Output = Result<(), ActionErrorKind>> + Send,
        P: Future<Output = Result<LoadedJob, ActionErrorKind>> + Send,
    {
        let Some(launcher) = &mut self.launcher else {
            // Receipts from older installers only removed the legacy plist.
            return remove_hook_plist(&self.path).await.map_err(Self::error);
        };
        let installed = installed_plist(&self.service_label, launcher.job_token.as_deref());
        let is_installed_plist = || matches!(read_plist::<LaunchctlHookPlist>(&self.path), Ok(Some(plist)) if plist == installed);
        if launcher.owns_plist && is_installed_plist() {
            let loaded = inspect().await.map_err(Self::error)?;
            if loaded == LoadedJob::Owned {
                unload().await.map_err(Self::error)?;
                if inspect().await.map_err(Self::error)? != LoadedJob::Absent {
                    return Err(Self::error(CreateNixHookServiceError::JobStillLoaded(
                        self.service_label.clone(),
                    )));
                }
            }
            if loaded != LoadedJob::Other {
                if is_installed_plist() {
                    remove_hook_plist(&self.path).await.map_err(Self::error)?;
                }
                launcher.owns_plist = false;
            }
        }
        // The stock legacy job runs the installer directly. Leave it loaded,
        // but remove the new files if no job or retained plist uses the helper.
        if launcher.owns_file
            && !retained_plist_uses_launcher(&self.path, &launcher.file.inner().path)
            && launcher.file_is_ready().await?
            && matches!(
                inspect().await.map_err(Self::error)?,
                LoadedJob::Absent | LoadedJob::Legacy
            )
            && !retained_plist_uses_launcher(&self.path, &launcher.file.inner().path)
        {
            launcher.file.try_revert().await?;
            launcher.owns_file = false;
        }
        if let Some(directory) = &launcher.directory {
            if matches!(
                directory.state,
                ActionState::Progress | ActionState::Completed
            ) {
                remove_empty_launcher_directory(&directory.inner().path)
                    .await
                    .map_err(Self::error)?;
                launcher.directory = None;
            }
        }
        Ok(())
    }
}

async fn remove_hook_plist(path: &Path) -> Result<(), ActionErrorKind> {
    crate::util::remove_file(path, OnMissing::Ignore)
        .await
        .map_err(|error| ActionErrorKind::Remove(path.to_owned(), error))
}

async fn remove_empty_launcher_directory(path: &Path) -> Result<(), ActionErrorKind> {
    // rmdir checks emptiness and removes the directory in one operation.
    match fs::remove_dir(path).await {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty | ErrorKind::NotADirectory
            ) =>
        {
            Ok(())
        },
        Err(error) => Err(ActionErrorKind::Remove(path.to_owned(), error)),
    }
}

fn launchctl_job_command(operation: &str, service_label: &str) -> Command {
    let mut command = Command::new("/bin/launchctl");
    command
        .process_group(0)
        .arg(operation)
        .arg(format!("{DARWIN_LAUNCHD_DOMAIN}/{service_label}"))
        .stdin(std::process::Stdio::null());
    command
}

async fn bootout_named_job(service_label: &str) -> Result<(), ActionErrorKind> {
    crate::execute_command(&mut launchctl_job_command("bootout", service_label)).await?;
    Ok(())
}

async fn inspect_loaded_job(
    service_label: &str,
    path: &Path,
    token: Option<&str>,
) -> Result<LoadedJob, ActionErrorKind> {
    let mut command = launchctl_job_command("print", service_label);
    let output = command
        .output()
        .await
        .map_err(|error| ActionErrorKind::command(&command, error))?;
    if output.status.code() == Some(LAUNCHCTL_SERVICE_NOT_FOUND) {
        return Ok(LoadedJob::Absent);
    }
    if !output.status.success() {
        return Err(ActionErrorKind::command_output(&command, output));
    }
    Ok(identify_loaded_job(
        &String::from_utf8_lossy(&output.stdout),
        path,
        token,
    ))
}

fn identify_loaded_job(output: &str, path: &Path, token: Option<&str>) -> LoadedJob {
    // launchctl print is diagnostic output. Recognize only the observed
    // service fields and exact argument list. Treat changes in format as
    // ambiguous rather than assuming ownership of a loaded job.
    let field = |prefix| output.lines().find_map(|line| line.strip_prefix(prefix));
    if field("\tpath = ").map(Path::new) != Some(path) {
        return LoadedJob::Other;
    }
    let mut lines = output.lines().skip_while(|line| *line != "\targuments = {");
    if lines.next().is_none() {
        return LoadedJob::Other;
    }
    let mut arguments = Vec::new();
    for line in &mut lines {
        if line == "\t}" {
            let program = field("\tprogram = ");
            if let Some(token) = token {
                if program == Some(LAUNCHER_PATH) && arguments == [LAUNCHER_PATH, token] {
                    return LoadedJob::Owned;
                }
            }
            if program == Some(LEGACY_PROGRAM)
                && arguments == [LEGACY_PROGRAM, "-c", LEGACY_COMMAND]
                && field("\tstdout path = ") == Some(HOOK_STDOUT)
                && field("\tstderr path = ") == Some(HOOK_STDERR)
            {
                return LoadedJob::Legacy;
            }
            break;
        }
        let Some(argument) = line.strip_prefix("\t\t") else {
            break;
        };
        arguments.push(argument);
    }
    LoadedJob::Other
}

fn retained_plist_uses_launcher(path: &Path, launcher: &Path) -> bool {
    match read_plist::<plist::Dictionary>(path) {
        Ok(None) => false,
        Ok(Some(plist)) => {
            let program = plist
                .get("Program")
                .or_else(|| plist.get("ProgramArguments")?.as_array()?.first())
                .and_then(plist::Value::as_string)
                .map(Path::new);
            let Some(program) = program else {
                return true;
            };
            if !program.is_absolute() || program == Path::new(LAUNCHER_PATH) || program == launcher
            {
                return true;
            }
            // Follow symlinks and compare device/inode so differently spelled
            // paths and hard links are recognized. Keep uncertain references.
            match (std_fs::metadata(program), std_fs::metadata(launcher)) {
                (Ok(program), Ok(launcher)) => {
                    (program.dev(), program.ino()) == (launcher.dev(), launcher.ino())
                },
                _ => true,
            }
        },
        Err(_) => true,
    }
}

fn launcher_directory_exists(path: &Path, owner: Option<u32>) -> Result<bool, ActionErrorKind> {
    let metadata = match std_fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(ActionErrorKind::GettingMetadata(path.into(), error)),
    };
    if !metadata.is_dir() {
        return Err(ActionErrorKind::PathWasNotDirectory(path.into()));
    }
    if let Some(owner) = owner {
        if metadata.uid() != owner {
            return Err(ActionErrorKind::PathUserMismatch(
                path.into(),
                metadata.uid(),
                owner,
            ));
        }
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o022 != 0 {
        return Err(ActionErrorKind::PathModeMismatch(
            path.into(),
            mode,
            mode & !0o022,
        ));
    }
    Ok(true)
}

fn read_plist<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, ActionErrorKind> {
    match std_fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(ActionErrorKind::PathWasNotFile(path.into()));
        },
        Ok(_) => {},
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ActionErrorKind::GettingMetadata(path.into(), error)),
    }
    let plist = plist::from_file(path)?;
    Ok(Some(plist))
}

fn write_plist(path: &Path, plist: &LaunchctlHookPlist) -> Result<(), ActionErrorKind> {
    let temporary = stage_plist(path, plist)?;
    let temporary_path = temporary.path().to_owned();
    temporary
        .persist(path)
        .map_err(|error| ActionErrorKind::Rename(temporary_path, path.into(), error.error))?;
    Ok(())
}

fn stage_plist(
    path: &Path,
    plist: &LaunchctlHookPlist,
) -> Result<tempfile::NamedTempFile, ActionErrorKind> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| ActionErrorKind::Open(path.into(), error))?;
    plist::to_writer_xml(temporary.as_file(), plist)?;
    temporary
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o644))
        .map_err(|error| ActionErrorKind::SetPermissions(0o644, path.into(), error))?;
    Ok(temporary)
}

fn generate_plist(service_label: &str) -> LaunchctlHookPlist {
    LaunchctlHookPlist {
        keep_alive: KeepAliveOpts {
            successful_exit: false,
        },
        label: service_label.into(),
        program_arguments: vec![LAUNCHER_PATH.into()],
        standard_error_path: HOOK_STDERR.into(),
        standard_out_path: HOOK_STDOUT.into(),
    }
}

fn valid_job_token(value: &str) -> bool {
    value
        .strip_prefix("nix-installer-repair-")
        .is_some_and(|suffix| {
            suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn installed_plist(service_label: &str, token: Option<&str>) -> LaunchctlHookPlist {
    let mut plist = generate_plist(service_label);
    if let Some(token) = token {
        plist.program_arguments.push(token.into());
    }
    plist
}

fn legacy_plist(service_label: &str) -> LaunchctlHookPlist {
    LaunchctlHookPlist {
        program_arguments: vec![LEGACY_PROGRAM.into(), "-c".into(), LEGACY_COMMAND.into()],
        ..generate_plist(service_label)
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
    #[error(
        "`{path}` exists and contains content different than expected. Consider removing the file."
    )]
    DifferentPlist {
        expected: LaunchctlHookPlist,
        discovered: LaunchctlHookPlist,
        path: PathBuf,
    },
    #[error("Launchd still lists the installer repair job `{0}` after bootout")]
    JobStillLoaded(String),
}

impl From<CreateNixHookServiceError> for ActionErrorKind {
    fn from(val: CreateNixHookServiceError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temporary_launcher(directory: &Path) -> HookLauncher {
        HookLauncher {
            owns_file: false,
            directory: None,
            owns_plist: false,
            job_token: None,
            file: CreateFile::plan(
                directory.join("nix-installer-repair"),
                None,
                None,
                Some(0o700),
                LAUNCHER_SCRIPT.into(),
                false,
            )
            .await
            .unwrap(),
        }
    }

    async fn skip_unload() -> Result<(), ActionErrorKind> {
        panic!("an unowned job must remain loaded");
    }

    #[tokio::test]
    async fn migrates_legacy_plist_and_replans_idempotently() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        write_plist(&path, &legacy_plist(SERVICE_LABEL)).unwrap();

        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap();
        assert_eq!(action.state, ActionState::Uncompleted);
        action.try_execute().await.unwrap();

        let plist = read_plist::<LaunchctlHookPlist>(&path).unwrap().unwrap();
        assert_eq!(plist.program_arguments[0], LAUNCHER_PATH);
        assert!(valid_job_token(&plist.program_arguments[1]));
        assert!(!plist.keep_alive.successful_exit);
        let launcher = directory.path().join("nix-installer-repair");
        assert_eq!(
            fs::read_to_string(&launcher).await.unwrap(),
            LAUNCHER_SCRIPT
        );
        assert_eq!(fs::metadata(&launcher).await.unwrap().mode() & 0o777, 0o700);

        let replan =
            CreateNixHookService::plan_at(path, temporary_launcher(directory.path()).await)
                .unwrap();
        let mut replan = replan;
        replan.try_execute().await.unwrap();
        let launcher = replan.inner().launcher.as_ref().unwrap();
        assert!(!launcher.owns_file && !launcher.owns_plist);
    }

    #[tokio::test]
    async fn leaves_legacy_plist_in_place_when_launcher_install_fails() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let legacy = legacy_plist(SERVICE_LABEL);
        write_plist(&path, &legacy).unwrap();
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap();
        let launcher = directory.path().join("nix-installer-repair");
        fs::write(&launcher, "unrelated file").await.unwrap();

        assert!(action.try_execute().await.is_err());
        action.try_revert().await.unwrap();
        assert_eq!(read_plist(&path).unwrap(), Some(legacy));
        assert_eq!(
            fs::read_to_string(&launcher).await.unwrap(),
            "unrelated file"
        );
    }

    #[tokio::test]
    async fn rejects_a_customized_legacy_plist() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let legacy = legacy_plist(SERVICE_LABEL);
        write_plist(&path, &legacy).unwrap();
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap();
        let mut customized = legacy;
        customized.standard_out_path = "/tmp/custom.log".into();
        write_plist(&path, &customized).unwrap();

        assert!(CreateNixHookService::plan_at(
            path.clone(),
            temporary_launcher(directory.path()).await
        )
        .is_err());
        assert!(action.try_execute().await.is_err());
        action.try_revert().await.unwrap();
        assert_eq!(read_plist(&path).unwrap(), Some(customized));
    }
    #[tokio::test]
    async fn rechecks_plist_after_installing_launcher() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let mut customized = legacy_plist(SERVICE_LABEL);
        write_plist(&path, &customized).unwrap();
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap()
                .action;

        action.launcher.as_mut().unwrap().install().await.unwrap();
        customized.standard_out_path = "/tmp/custom.log".into();
        write_plist(&path, &customized).unwrap();

        assert!(action.install_named_plist().is_err());
        action
            .revert_with(skip_unload, || std::future::ready(Ok(LoadedJob::Absent)))
            .await
            .unwrap();
        assert_eq!(read_plist(&path).unwrap(), Some(customized));
        assert!(!directory.path().join("nix-installer-repair").exists());
    }

    #[tokio::test]
    async fn recreates_a_launcher_removed_after_planning() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let launcher_path = directory.path().join("nix-installer-repair");
        temporary_launcher(directory.path())
            .await
            .install()
            .await
            .unwrap();
        write_plist(&path, &legacy_plist(SERVICE_LABEL)).unwrap();
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap();

        fs::remove_file(&launcher_path).await.unwrap();
        action.try_execute().await.unwrap();
        assert_eq!(
            fs::read_to_string(&launcher_path).await.unwrap(),
            LAUNCHER_SCRIPT
        );
        assert!(action.inner().plist_state().unwrap() == PlistState::Named);
    }

    #[tokio::test]
    async fn keeps_a_named_hook_installed_after_planning() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        write_plist(&path, &legacy_plist(SERVICE_LABEL)).unwrap();
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap();
        temporary_launcher(directory.path())
            .await
            .install()
            .await
            .unwrap();
        let named = generate_plist(SERVICE_LABEL);
        write_plist(&path, &named).unwrap();

        action.try_execute().await.unwrap();
        action.try_revert().await.unwrap();
        assert_eq!(read_plist(&path).unwrap(), Some(named));
        assert_eq!(
            fs::read_to_string(directory.path().join("nix-installer-repair"))
                .await
                .unwrap(),
            LAUNCHER_SCRIPT,
        );
    }
    #[tokio::test]
    async fn retries_a_saved_plan_after_plist_publication_failed() {
        let directory = tempfile::tempdir().unwrap();
        let plist_dir = directory.path().join("LaunchDaemons");
        let path = plist_dir.join("hook.plist");
        let launcher_path = directory.path().join("nix-installer-repair");
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap();

        assert!(action.try_execute().await.is_err());
        assert_eq!(
            fs::read_to_string(&launcher_path).await.unwrap(),
            LAUNCHER_SCRIPT
        );
        let saved = serde_json::to_vec(&action).unwrap();
        let mut resumed: StatefulAction<CreateNixHookService> =
            serde_json::from_slice(&saved).unwrap();
        assert!(resumed.try_execute().await.is_err());
        resumed
            .action
            .revert_with(skip_unload, || std::future::ready(Ok(LoadedJob::Absent)))
            .await
            .unwrap();
        assert!(!path.exists());
        assert!(!launcher_path.exists());
    }
    #[tokio::test]
    async fn unloads_owned_named_job_before_removing_launcher() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let launcher_path = directory.path().join("nix-installer-repair");
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap();
        action.try_execute().await.unwrap();
        let unloaded = std::cell::Cell::new(false);
        action
            .action
            .revert_with(
                || {
                    assert!(launcher_path.exists());
                    unloaded.set(true);
                    std::future::ready(Ok(()))
                },
                || {
                    std::future::ready(Ok(if unloaded.get() {
                        LoadedJob::Absent
                    } else {
                        LoadedJob::Owned
                    }))
                },
            )
            .await
            .unwrap();

        assert!(unloaded.get());
        assert!(!path.exists());
        assert!(!launcher_path.exists());
    }

    #[tokio::test]
    async fn keeps_a_launcher_used_by_a_customized_plist() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let launcher_path = directory.path().join("nix-installer-repair");
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap()
                .action;
        action.launcher.as_mut().unwrap().install().await.unwrap();

        let mut customized = generate_plist(SERVICE_LABEL);
        customized.standard_out_path = "/tmp/custom.log".into();
        write_plist(&path, &customized).unwrap();
        assert!(action.install_named_plist().is_err());
        action
            .revert_with(skip_unload, || async {
                panic!("a helper used by a retained plist needs no runtime check")
            })
            .await
            .unwrap();
        assert!(launcher_path.exists());

        // Program overrides ProgramArguments[0], and the strict install-time
        // parser intentionally rejects unknown keys on customized plists.
        let mut customized: plist::Dictionary = read_plist(&path).unwrap().unwrap();
        customized.insert(
            "ProgramArguments".into(),
            plist::Value::Array(vec![plist::Value::String("custom-argv0".into())]),
        );
        customized.insert("Program".into(), plist::Value::String(LAUNCHER_PATH.into()));
        plist::to_file_xml(&path, &customized).unwrap();
        action
            .revert_with(skip_unload, || async {
                panic!("a helper used by a retained plist needs no runtime check")
            })
            .await
            .unwrap();
        assert!(launcher_path.exists());
        assert_eq!(read_plist(&path).unwrap(), Some(customized));
    }
    #[tokio::test]
    async fn saved_named_plan_recreates_a_missing_launcher() {
        let dir = tempfile::tempdir().unwrap();
        let plist_path = dir.path().join("hook.plist");
        let launcher_path = dir.path().join("nix-installer-repair");
        let named = generate_plist(SERVICE_LABEL);
        write_plist(&plist_path, &named).unwrap();
        temporary_launcher(dir.path())
            .await
            .install()
            .await
            .unwrap();
        let plan =
            CreateNixHookService::plan_at(plist_path.clone(), temporary_launcher(dir.path()).await)
                .unwrap();
        let saved = serde_json::to_vec(&plan).unwrap();

        fs::remove_file(&launcher_path).await.unwrap();
        let mut resumed: StatefulAction<CreateNixHookService> =
            serde_json::from_slice(&saved).unwrap();
        resumed.try_execute().await.unwrap();

        assert_eq!(
            fs::read_to_string(&launcher_path).await.unwrap(),
            LAUNCHER_SCRIPT
        );
        assert_eq!(read_plist(&plist_path).unwrap(), Some(named));
    }

    #[tokio::test]
    async fn rollback_keeps_a_loaded_job_when_its_plist_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let plist_path = dir.path().join("hook.plist");
        let launcher_path = dir.path().join("nix-installer-repair");
        let mut plan =
            CreateNixHookService::plan_at(plist_path.clone(), temporary_launcher(dir.path()).await)
                .unwrap();
        plan.try_execute().await.unwrap();
        let mut replacement = generate_plist(SERVICE_LABEL);
        replacement.program_arguments = vec!["/usr/bin/true".into()];
        write_plist(&plist_path, &replacement).unwrap();

        plan.action
            .revert_with(skip_unload, || std::future::ready(Ok(LoadedJob::Other)))
            .await
            .unwrap();
        assert_eq!(read_plist(&plist_path).unwrap(), Some(replacement));
        assert!(launcher_path.exists());
    }

    #[tokio::test]
    async fn rollback_keeps_a_loaded_jobs_launcher_after_its_plist_disappears() {
        let dir = tempfile::tempdir().unwrap();
        let plist_path = dir.path().join("hook.plist");
        let launcher_path = dir.path().join("nix-installer-repair");
        write_plist(&plist_path, &generate_plist(SERVICE_LABEL)).unwrap();
        let mut plan =
            CreateNixHookService::plan_at(plist_path.clone(), temporary_launcher(dir.path()).await)
                .unwrap();
        plan.try_execute().await.unwrap();
        fs::remove_file(&plist_path).await.unwrap();

        plan.action
            .revert_with(skip_unload, || std::future::ready(Ok(LoadedJob::Other)))
            .await
            .unwrap();
        assert!(launcher_path.exists());
    }

    #[tokio::test]
    async fn legacy_receipt_removes_the_plist_without_contacting_launchd() {
        let dir = tempfile::tempdir().unwrap();
        let plist_path = dir.path().join("hook.plist");
        write_plist(&plist_path, &legacy_plist(SERVICE_LABEL)).unwrap();
        let mut action: CreateNixHookService = serde_json::from_value(serde_json::json!({
            "action_name": "create_nix_hook_service",
            "path": plist_path,
            "service_label": SERVICE_LABEL,
            "needs_bootout": false,
        }))
        .unwrap();
        action
            .revert_with(skip_unload, || async {
                panic!("legacy receipts do not query launchd on uninstall")
            })
            .await
            .unwrap();
        assert!(!plist_path.exists());
    }
    #[test]
    fn identifies_only_the_loaded_job_for_this_plan() {
        let path = Path::new("/Library/LaunchDaemons/hook.plist");
        let token = "nix-installer-repair-00000000000000000000000000000001";
        let loaded = format!(
            "system/{SERVICE_LABEL} = {{\n\tpath = {}\n\tprogram = {LAUNCHER_PATH}\n\targuments = {{\n\t\t{LAUNCHER_PATH}\n\t\t{token}\n\t}}\n}}",
            path.display()
        );
        assert_eq!(
            identify_loaded_job(&loaded, path, Some(token)),
            LoadedJob::Owned
        );
        assert_eq!(
            identify_loaded_job(&loaded, path, Some("other")),
            LoadedJob::Other
        );
        assert_eq!(
            identify_loaded_job(&loaded, Path::new("/tmp/hook.plist"), Some(token)),
            LoadedJob::Other
        );
        assert_eq!(identify_loaded_job(&loaded, path, None), LoadedJob::Other);
    }

    #[tokio::test]
    async fn preserves_a_different_loaded_job_even_with_the_owned_plist_on_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap();
        action.try_execute().await.unwrap();

        action
            .action
            .revert_with(skip_unload, || std::future::ready(Ok(LoadedJob::Other)))
            .await
            .unwrap();

        assert!(path.exists());
        assert!(directory.path().join("nix-installer-repair").exists());
    }

    #[tokio::test]
    async fn preserves_owned_files_if_bootout_does_not_remove_the_job() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap();
        action.try_execute().await.unwrap();

        assert!(action
            .action
            .revert_with(
                || std::future::ready(Ok(())),
                || std::future::ready(Ok(LoadedJob::Owned)),
            )
            .await
            .is_err());
        assert!(path.exists());
        assert!(directory.path().join("nix-installer-repair").exists());
    }

    #[tokio::test]
    async fn recreates_the_helper_directory_and_tolerates_its_absence_on_revert() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("PrivilegedHelperTools");
        fs::create_dir(&parent).await.unwrap();
        let path = directory.path().join("hook.plist");
        let plan =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(&parent).await).unwrap();
        fs::remove_dir(&parent).await.unwrap();

        let saved = serde_json::to_vec(&plan).unwrap();
        let mut action: StatefulAction<CreateNixHookService> =
            serde_json::from_slice(&saved).unwrap();
        action.try_execute().await.unwrap();
        let helper = parent.join("nix-installer-repair");
        assert_eq!(fs::read_to_string(&helper).await.unwrap(), LAUNCHER_SCRIPT);

        fs::remove_file(&helper).await.unwrap();
        fs::remove_dir(&parent).await.unwrap();
        action
            .action
            .revert_with(skip_unload, || std::future::ready(Ok(LoadedJob::Absent)))
            .await
            .unwrap();
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn keeps_a_helper_referenced_through_a_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap()
                .action;
        action.launcher.as_mut().unwrap().install().await.unwrap();
        let helper = directory.path().join("nix-installer-repair");
        let alias = directory.path().join("helper-link");
        std::os::unix::fs::symlink(&helper, &alias).unwrap();
        let mut retained = generate_plist(SERVICE_LABEL);
        retained.program_arguments = vec![alias.to_str().unwrap().into()];
        write_plist(&path, &retained).unwrap();

        action
            .revert_with(skip_unload, || std::future::ready(Ok(LoadedJob::Absent)))
            .await
            .unwrap();
        assert!(helper.exists());

        // Relative programs use launchd's context. An identically spelled
        // file in the installer's working directory is not evidence.
        let cwd_file = Path::new("Cargo.toml");
        assert!(cwd_file.is_file());
        retained.program_arguments = vec![cwd_file.to_str().unwrap().into()];
        write_plist(&path, &retained).unwrap();
        action
            .revert_with(skip_unload, || std::future::ready(Ok(LoadedJob::Absent)))
            .await
            .unwrap();
        assert!(helper.exists());
    }
    #[tokio::test]
    async fn keeps_a_helper_referenced_by_a_plist_created_during_the_live_probe() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .unwrap()
                .action;
        action.launcher.as_mut().unwrap().install().await.unwrap();

        action
            .revert_with(skip_unload, || {
                write_plist(&path, &generate_plist(SERVICE_LABEL)).unwrap();
                std::future::ready(Ok(LoadedJob::Absent))
            })
            .await
            .unwrap();

        assert!(directory.path().join("nix-installer-repair").exists());
    }
    #[tokio::test]
    async fn removes_migrated_files_while_the_stock_legacy_job_is_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hook.plist");
        write_plist(&path, &legacy_plist(SERVICE_LABEL)).unwrap();
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(dir.path()).await)
                .unwrap();
        action.try_execute().await.unwrap();
        let loaded = format!(
            "system/{SERVICE_LABEL} = {{\n\tpath = {}\n\tprogram = {LEGACY_PROGRAM}\n\targuments = {{\n\t\t{LEGACY_PROGRAM}\n\t\t-c\n\t\t{LEGACY_COMMAND}\n\t}}\n\tstdout path = {HOOK_STDOUT}\n\tstderr path = {HOOK_STDERR}\n}}",
            path.display()
        );
        let token = action.inner().launcher.as_ref().unwrap().job_token.clone();
        let job = identify_loaded_job(&loaded, &path, token.as_deref());
        assert_eq!(job, LoadedJob::Legacy);
        assert_eq!(
            identify_loaded_job(
                &loaded.replace(HOOK_STDOUT, "/tmp/custom.log"),
                &path,
                token.as_deref()
            ),
            LoadedJob::Other
        );
        action
            .action
            .revert_with(skip_unload, || std::future::ready(Ok(job)))
            .await
            .unwrap();
        assert!(!path.exists());
        assert!(!dir.path().join("nix-installer-repair").exists());
    }

    #[tokio::test]
    async fn preserves_helper_when_job_loads_after_initial_absence_check() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hook.plist");
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(dir.path()).await)
                .unwrap();
        action.try_execute().await.unwrap();

        action
            .action
            .revert_with(skip_unload, || {
                let job = if path.exists() {
                    LoadedJob::Absent
                } else {
                    LoadedJob::Other
                };
                std::future::ready(Ok(job))
            })
            .await
            .unwrap();
        assert!(!path.exists());
        assert!(dir.path().join("nix-installer-repair").exists());
    }

    #[tokio::test]
    async fn removes_shared_directory_only_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared");
        fs::create_dir(&path).await.unwrap();
        let unrelated = path.join("other-helper");
        fs::write(&unrelated, "other installer").await.unwrap();

        remove_empty_launcher_directory(&path).await.unwrap();
        assert_eq!(
            fs::read_to_string(&unrelated).await.unwrap(),
            "other installer"
        );
        fs::remove_file(unrelated).await.unwrap();
        remove_empty_launcher_directory(&path).await.unwrap();
        assert!(!path.exists());
        remove_empty_launcher_directory(&path).await.unwrap();

        std::os::unix::fs::symlink(dir.path(), &path).unwrap();
        remove_empty_launcher_directory(&path).await.unwrap();
        assert!(fs::symlink_metadata(&path)
            .await
            .unwrap()
            .file_type()
            .is_symlink());
    }
}
