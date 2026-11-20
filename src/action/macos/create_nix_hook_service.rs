use serde::{Deserialize, Serialize};
use tracing::{span, Span};

use std::{
    io::ErrorKind,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::fs;

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
    directory: Option<StatefulAction<CreateDirectory>>,
    file: StatefulAction<CreateFile>,
    #[serde(default)]
    owns_plist: bool,
}

impl CreateNixHookService {
    async fn plan_launcher() -> Result<HookLauncher, ActionError> {
        let directory = Path::new(LAUNCHER_DIRECTORY);
        let directory_action = match fs::symlink_metadata(directory).await {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    return Err(Self::error(ActionErrorKind::PathWasNotDirectory(
                        directory.into(),
                    )));
                }
                if metadata.uid() != 0 {
                    return Err(Self::error(ActionErrorKind::PathUserMismatch(
                        directory.into(),
                        metadata.uid(),
                        0,
                    )));
                }
                let mode = metadata.permissions().mode() & 0o777;
                if mode & 0o022 != 0 {
                    return Err(Self::error(ActionErrorKind::PathModeMismatch(
                        directory.into(),
                        mode,
                        mode & !0o022,
                    )));
                }
                None
            },
            Err(error) if error.kind() == ErrorKind::NotFound => Some(
                CreateDirectory::plan(
                    directory,
                    Some("root".into()),
                    Some("wheel".into()),
                    Some(0o755),
                    false,
                )
                .await?,
            ),
            Err(error) => {
                return Err(Self::error(ActionErrorKind::GettingMetadata(
                    directory.into(),
                    error,
                )));
            },
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
        .await
    }

    async fn plan_at(
        path: PathBuf,
        launcher: HookLauncher,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let launcher_ready =
            launcher.directory.is_none() && launcher.file.state == ActionState::Completed;
        let mut this = Self {
            path,
            service_label: SERVICE_LABEL.into(),
            needs_bootout: false,
            launcher: Some(launcher),
        };

        let owns_plist = this.has_named_plist().await?;
        if let Some(launcher) = &mut this.launcher {
            launcher.owns_plist = owns_plist;
        }
        if owns_plist && launcher_ready {
            return Ok(StatefulAction::completed(this));
        }

        Ok(StatefulAction::uncompleted(this))
    }

    async fn has_named_plist(&self) -> Result<bool, ActionError> {
        let Some(discovered) = read_plist(&self.path).await.map_err(Self::error)? else {
            return Ok(false);
        };
        let expected = generate_plist(&self.service_label);
        if discovered == expected {
            Ok(true)
        } else if discovered == legacy_plist(&self.service_label) {
            Ok(false)
        } else {
            Err(Self::error(CreateNixHookServiceError::DifferentPlist {
                expected,
                discovered,
                path: self.path.clone(),
            }))
        }
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
        let already_named = self.launcher.is_some() && self.has_named_plist().await?;
        if let Some(launcher) = &mut self.launcher {
            if let Some(directory) = &mut launcher.directory {
                directory.try_execute().await?;
            }
            if let Err(error) = launcher.file.try_execute().await {
                if matches!(error.kind(), ActionErrorKind::Open(_, io) if io.kind() == ErrorKind::AlreadyExists)
                {
                    // create_new failed before acquiring this path.
                    launcher.file.state = ActionState::Uncompleted;
                }
                return Err(error);
            }

            // An already loaded legacy job can finish with its original
            // command. launchd reads the replaced plist on its next load.
            if !already_named {
                write_plist(&self.path, &generate_plist(&self.service_label))
                    .map_err(Self::error)?;
            }
            launcher.owns_plist = true;
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
        let mut description = vec![ActionDescription::new(
            format!("Delete file `{}`", self.path.display()),
            vec![format!("Delete file `{}`", self.path.display())],
        )];
        if let Some(launcher) = &self.launcher {
            description.extend(launcher.file.describe_revert());
            if let Some(directory) = &launcher.directory {
                description.extend(directory.describe_revert());
            }
        }
        description
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        // A failed migration may have left the original service in place.
        // Keep it available if the named plist was never installed.
        let owns_plist = match &self.launcher {
            None => true,
            Some(launcher) if launcher.owns_plist => true,
            Some(_) => matches!(
                read_plist(&self.path).await,
                Ok(Some(plist)) if plist == generate_plist(&self.service_label)
            ),
        };
        if owns_plist {
            crate::action::macos::retry_bootout(DARWIN_LAUNCHD_DOMAIN, &self.service_label)
                .await
                .map_err(Self::error)?;
            crate::util::remove_file(&self.path, OnMissing::Ignore)
                .await
                .map_err(|e| Self::error(ActionErrorKind::Remove(self.path.to_owned(), e)))?;
        }
        if let Some(launcher) = &mut self.launcher {
            launcher.file.try_revert().await?;
            if let Some(directory) = &mut launcher.directory {
                directory.try_revert().await?;
            }
        }

        Ok(())
    }
}

async fn read_plist(path: &Path) -> Result<Option<LaunchctlHookPlist>, ActionErrorKind> {
    match fs::read(path).await {
        Ok(bytes) => Ok(Some(plist::from_bytes(&bytes)?)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ActionErrorKind::Read(path.into(), error)),
    }
}

fn write_plist(path: &Path, plist: &LaunchctlHookPlist) -> Result<(), ActionErrorKind> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|error| ActionErrorKind::Open(path.into(), error))?;
    plist::to_writer_xml(temporary.as_file(), plist)?;
    temporary
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o644))
        .map_err(|error| ActionErrorKind::SetPermissions(0o644, path.into(), error))?;
    let temporary_path = temporary.path().to_owned();
    temporary
        .persist(path)
        .map_err(|error| ActionErrorKind::Rename(temporary_path, path.into(), error.error))?;
    Ok(())
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

    #[tokio::test]
    async fn migrates_legacy_plist_and_replans_idempotently() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        write_plist(&path, &legacy_plist(SERVICE_LABEL)).unwrap();

        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .await
                .unwrap();
        assert_eq!(action.state, ActionState::Uncompleted);
        action.try_execute().await.unwrap();

        let plist = read_plist(&path).await.unwrap().unwrap();
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
                .await
                .unwrap();
        assert_eq!(replan.state, ActionState::Completed);
    }

    #[tokio::test]
    async fn leaves_legacy_plist_in_place_when_launcher_install_fails() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hook.plist");
        let legacy = legacy_plist(SERVICE_LABEL);
        write_plist(&path, &legacy).unwrap();
        let mut action =
            CreateNixHookService::plan_at(path.clone(), temporary_launcher(directory.path()).await)
                .await
                .unwrap();
        let launcher = directory.path().join("nix-installer-repair");
        fs::write(&launcher, "unrelated file").await.unwrap();

        assert!(action.try_execute().await.is_err());
        action.try_revert().await.unwrap();
        assert_eq!(read_plist(&path).await.unwrap(), Some(legacy));
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
                .await
                .unwrap();
        let mut customized = legacy;
        customized.standard_out_path = "/tmp/custom.log".into();
        write_plist(&path, &customized).unwrap();

        assert!(CreateNixHookService::plan_at(
            path.clone(),
            temporary_launcher(directory.path()).await
        )
        .await
        .is_err());
        assert!(action.try_execute().await.is_err());
        action.try_revert().await.unwrap();
        assert_eq!(read_plist(&path).await.unwrap(), Some(customized));
    }
}
