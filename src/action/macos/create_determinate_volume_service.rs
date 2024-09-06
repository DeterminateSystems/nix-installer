use serde::{Deserialize, Serialize};
use tracing::{span, Span};

use std::path::{Path, PathBuf};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::AsyncWriteExt,
    process::Command,
};

use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

use super::DARWIN_LAUNCHD_DOMAIN;

/** Create a plist for a `launchctl` service to mount the volume
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_determinate_volume_service")]
pub struct CreateDeterminateVolumeService {
    path: PathBuf,
    mount_service_label: String,
    needs_bootout: bool,
}

impl CreateDeterminateVolumeService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        mount_service_label: impl Into<String>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref().to_path_buf();
        let mount_service_label = mount_service_label.into();
        let mut this = Self {
            path,
            mount_service_label,
            needs_bootout: false,
        };

        // If the service is currently loaded or running, we need to unload it during execute (since we will then recreate it and reload it)
        // This `launchctl` command may fail if the service isn't loaded
        let mut check_loaded_command = Command::new("launchctl");
        check_loaded_command.arg("print");
        check_loaded_command.arg(format!("system/{}", this.mount_service_label));
        tracing::trace!(
            command = format!("{:?}", check_loaded_command.as_std()),
            "Executing"
        );
        let check_loaded_output = check_loaded_command
            .status()
            .await
            .map_err(|e| ActionErrorKind::command(&check_loaded_command, e))
            .map_err(Self::error)?;

        this.needs_bootout = check_loaded_output.success();

        if this.needs_bootout {
            tracing::debug!(
                "Detected loaded service `{}` which needs unload before replacing `{}`",
                this.mount_service_label,
                this.path.display(),
            );
        }

        if this.path.exists() {
            let discovered_plist: LaunchctlMountPlist =
                plist::from_file(&this.path).map_err(Self::error)?;

            let expected_plist = generate_mount_plist(&this.mount_service_label)
                .await
                .map_err(Self::error)?;
            if discovered_plist != expected_plist {
                tracing::trace!(
                    ?discovered_plist,
                    ?expected_plist,
                    "Parsed plists not equal"
                );
                return Err(Self::error(
                    CreateDeterminateVolumeServiceError::DifferentPlist {
                        expected: expected_plist,
                        discovered: discovered_plist,
                        path: this.path.clone(),
                    },
                ));
            }

            tracing::debug!("Creating file `{}` already complete", this.path.display());
            return Ok(StatefulAction::completed(this));
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_determinate_volume_service")]
impl Action for CreateDeterminateVolumeService {
    fn action_tag() -> ActionTag {
        ActionTag("create_determinate_volume_service")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "{maybe_unload} a `launchctl` plist to mount the APFS volume `{path}`",
            path = self.path.display(),
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
            "create_determinate_volume_service",
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
        let Self {
            path,
            mount_service_label,
            needs_bootout,
        } = self;

        if *needs_bootout {
            crate::action::macos::retry_bootout(DARWIN_LAUNCHD_DOMAIN, &path)
                .await
                .map_err(Self::error)?;
        }

        let generated_plist = generate_mount_plist(mount_service_label)
            .await
            .map_err(Self::error)?;

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
        vec![ActionDescription::new(
            format!("Delete file `{}`", self.path.display()),
            vec![format!("Delete file `{}`", self.path.display())],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        remove_file(&self.path)
            .await
            .map_err(|e| Self::error(ActionErrorKind::Remove(self.path.to_owned(), e)))?;

        Ok(())
    }
}

/// This function must be able to operate at both plan and execute time.
async fn generate_mount_plist(
    mount_service_label: &str,
) -> Result<LaunchctlMountPlist, ActionErrorKind> {
    let mount_plist = LaunchctlMountPlist {
        run_at_load: true,
        label: mount_service_label.into(),
        program_arguments: vec![
            "/usr/local/bin/determinate-nixd".into(),
            "--stop-after".into(),
            "mount".into(),
        ],
        standard_out_path: "/var/log/determinate-nixd-mount.log".into(),
        standard_error_path: "/var/log/determinate-nixd-mount.log".into(),
    };

    Ok(mount_plist)
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct LaunchctlMountPlist {
    run_at_load: bool,
    label: String,
    program_arguments: Vec<String>,
    standard_error_path: String,
    standard_out_path: String,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum CreateDeterminateVolumeServiceError {
    #[error("`{path}` contents differs, planned `{expected:?}`, discovered `{discovered:?}`")]
    DifferentPlist {
        expected: LaunchctlMountPlist,
        discovered: LaunchctlMountPlist,
        path: PathBuf,
    },
}

impl From<CreateDeterminateVolumeServiceError> for ActionErrorKind {
    fn from(val: CreateDeterminateVolumeServiceError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}
