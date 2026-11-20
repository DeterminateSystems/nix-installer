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
}

#[derive(PartialEq, Eq)]
enum PlistState {
    Missing,
    Legacy,
    Named,
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
            if let Some(directory) = &mut self.directory {
                directory.try_execute().await?;
            }
        }
        if !self.file_is_ready().await? {
            self.file.inner().create_atomically()?;
            self.owns_file = true;
        }
        self.file.state = ActionState::Completed;
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
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        Self::plan_at(
            format!("/Library/LaunchDaemons/{SERVICE_LABEL}.plist").into(),
            Self::plan_launcher().await?,
        )
    }

    fn plan_at(path: PathBuf, launcher: HookLauncher) -> Result<StatefulAction<Self>, ActionError> {
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
        if discovered == expected {
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
        let temporary =
            stage_plist(&self.path, &generate_plist(&self.service_label)).map_err(Self::error)?;
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
        self.revert_with(
            || crate::action::macos::retry_bootout(DARWIN_LAUNCHD_DOMAIN, &service),
            || named_job_is_unloaded(&service),
        )
        .await
    }
}

impl CreateNixHookService {
    async fn revert_with<U, P>(
        &mut self,
        unload: impl FnOnce() -> U,
        is_unloaded: impl FnOnce() -> P,
    ) -> Result<(), ActionError>
    where
        U: Future<Output = Result<(), ActionErrorKind>> + Send,
        P: Future<Output = bool> + Send,
    {
        let Some(launcher) = &mut self.launcher else {
            // Receipts from older installers only removed the legacy plist.
            return remove_hook_plist(&self.path).await.map_err(Self::error);
        };
        let installed = generate_plist(&self.service_label);
        let is_installed_plist = || matches!(read_plist::<LaunchctlHookPlist>(&self.path), Ok(Some(plist)) if plist == installed);
        if launcher.owns_plist && is_installed_plist() {
            unload().await.map_err(Self::error)?;
            if is_installed_plist() {
                remove_hook_plist(&self.path).await.map_err(Self::error)?;
            }
            launcher.owns_plist = false;
        }
        // A replaced plist is not evidence about the job still held by
        // launchd. Do not stop an ambiguous job or remove a helper it may use.
        if launcher.owns_file
            && !retained_plist_uses_launcher(&self.path)
            && is_unloaded().await
            && launcher.file_is_ready().await?
        {
            launcher.file.try_revert().await?;
            launcher.owns_file = false;
        }
        if let Some(directory) = &mut launcher.directory {
            directory.try_revert().await?;
        }
        Ok(())
    }
}

async fn remove_hook_plist(path: &Path) -> Result<(), ActionErrorKind> {
    crate::util::remove_file(path, OnMissing::Ignore)
        .await
        .map_err(|error| ActionErrorKind::Remove(path.to_owned(), error))
}

async fn named_job_is_unloaded(service_label: &str) -> bool {
    // Use the service-not-found exit status. The printed fields are diagnostic
    // output, and other errors do not establish that a job is absent.
    Command::new("/bin/launchctl")
        .arg("print")
        .arg(format!("{DARWIN_LAUNCHD_DOMAIN}/{service_label}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.code() == Some(LAUNCHCTL_SERVICE_NOT_FOUND))
}

fn retained_plist_uses_launcher(path: &Path) -> bool {
    match read_plist::<plist::Dictionary>(path) {
        Ok(None) => false,
        Ok(Some(plist)) => {
            let program = plist
                .get("Program")
                .or_else(|| plist.get("ProgramArguments")?.as_array()?.first());
            // If the retained job cannot be inspected, leave its helper in place.
            program
                .and_then(plist::Value::as_string)
                .is_none_or(|program| Path::new(program) == Path::new(LAUNCHER_PATH))
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
        standard_error_path: "/nix/.nix-installer-hook.err.log".into(),
        standard_out_path: "/nix/.nix-installer-hook.out.log".into(),
    }
}

fn legacy_plist(service_label: &str) -> LaunchctlHookPlist {
    LaunchctlHookPlist {
        program_arguments: vec![
            "/bin/sh".into(),
            "-c".into(),
            "/bin/wait4path /nix/nix-installer && /nix/nix-installer repair".into(),
        ],
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
        assert_eq!(plist.program_arguments, [LAUNCHER_PATH]);
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
            .revert_with(skip_unload, || std::future::ready(true))
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
        assert_eq!(
            read_plist(&path).unwrap(),
            Some(generate_plist(SERVICE_LABEL))
        );
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
            .revert_with(skip_unload, || std::future::ready(true))
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
                || std::future::ready(unloaded.get()),
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
            .revert_with(skip_unload, || std::future::ready(false))
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
            .revert_with(skip_unload, || std::future::ready(false))
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
}
