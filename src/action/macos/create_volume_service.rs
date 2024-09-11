use serde::{Deserialize, Serialize};
use tracing::{span, Span};

use std::path::{Path, PathBuf};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::AsyncWriteExt,
    process::Command,
};

use crate::action::{
    macos::DARWIN_LAUNCHD_DOMAIN, Action, ActionDescription, ActionError, ActionErrorKind,
    ActionTag, StatefulAction,
};

use super::get_uuid_for_label;

/** Create a plist for a `launchctl` service to mount the given `apfs_volume_label` on the given `mount_point`.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_volume_service")]
pub struct CreateVolumeService {
    pub(crate) path: PathBuf,
    apfs_volume_label: String,
    mount_service_label: String,
    mount_point: PathBuf,
    encrypt: bool,
    needs_bootout: bool,
}

impl CreateVolumeService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        mount_service_label: impl Into<String>,
        apfs_volume_label: String,
        mount_point: impl AsRef<Path>,
        encrypt: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref().to_path_buf();
        let mount_point = mount_point.as_ref().to_path_buf();
        let mount_service_label = mount_service_label.into();
        let mut this = Self {
            path,
            apfs_volume_label,
            mount_service_label,
            mount_point,
            encrypt,
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
            .output()
            .await
            .map_err(|e| ActionErrorKind::command(&check_loaded_command, e))
            .map_err(Self::error)?;
        this.needs_bootout = check_loaded_output.status.success();
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
            match get_uuid_for_label(&this.apfs_volume_label)
                .await
                .map_err(Self::error)?
            {
                Some(uuid) => {
                    let expected_plist = generate_mount_plist(
                        &this.mount_service_label,
                        &this.apfs_volume_label,
                        uuid,
                        &this.mount_point,
                        encrypt,
                    )
                    .await
                    .map_err(Self::error)?;
                    if discovered_plist != expected_plist {
                        tracing::trace!(
                            ?discovered_plist,
                            ?expected_plist,
                            "Parsed plists not equal"
                        );
                        return Err(Self::error(CreateVolumeServiceError::DifferentPlist {
                            expected: expected_plist,
                            discovered: discovered_plist,
                            path: this.path.clone(),
                        }));
                    }

                    tracing::debug!("Creating file `{}` already complete", this.path.display());
                    return Ok(StatefulAction::completed(this));
                },
                None => {
                    tracing::debug!(
                        "Detected existing service path `{}` but could not detect a UUID for the volume",
                        this.path.display()
                    );

                    // If there is already a line in `/etc/fstab` with `/nix` in it, the user will likely experience an error during execute,
                    // so check if there exists a line, which is not a comment, that contains `/nix`
                    let fstab = PathBuf::from("/etc/fstab");
                    if fstab.exists() {
                        let contents = tokio::fs::read_to_string(&fstab)
                            .await
                            .map_err(|e| Self::error(ActionErrorKind::Read(fstab, e)))?;
                        for line in contents.lines() {
                            if line.starts_with('#') {
                                continue;
                            }
                            let split = line.split_whitespace();
                            for item in split {
                                if item == "/nix" {
                                    return Err(Self::error(CreateVolumeServiceError::VolumeDoesNotExistButVolumeServiceAndFstabEntryDoes(this.path.clone(), this.apfs_volume_label)));
                                }
                            }
                        }
                    }
                },
            }
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_volume_service")]
impl Action for CreateVolumeService {
    fn action_tag() -> ActionTag {
        ActionTag("create_volume_service")
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
            "create_volume_service",
            path = tracing::field::display(self.path.display()),
            buf = tracing::field::Empty,
        );

        if tracing::enabled!(tracing::Level::TRACE) {
            span.record("buf", &self.apfs_volume_label);
        }
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
            apfs_volume_label,
            mount_point,
            encrypt,
            needs_bootout,
        } = self;

        if *needs_bootout {
            crate::action::macos::retry_bootout(DARWIN_LAUNCHD_DOMAIN, &mount_service_label, &path)
                .await
                .map_err(Self::error)?;
        }

        let uuid = match get_uuid_for_label(apfs_volume_label)
            .await
            .map_err(Self::error)?
        {
            Some(uuid) => uuid,
            None => {
                return Err(Self::error(CreateVolumeServiceError::CannotDetermineUuid(
                    apfs_volume_label.to_string(),
                )))
            },
        };
        let generated_plist = generate_mount_plist(
            mount_service_label,
            apfs_volume_label,
            uuid,
            mount_point,
            *encrypt,
        )
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
    apfs_volume_label: &str,
    uuid: uuid::Uuid,
    mount_point: &Path,
    encrypt: bool,
) -> Result<LaunchctlMountPlist, ActionErrorKind> {
    let apfs_volume_label_with_quotes = format!("\"{apfs_volume_label}\"");
    // The official Nix scripts uppercase the UUID, so we do as well for compatibility.
    let uuid_string = uuid.to_string().to_uppercase();
    let mount_command = if encrypt {
        let encrypted_command = format!("/usr/bin/security find-generic-password -s {apfs_volume_label_with_quotes} -w |  /usr/sbin/diskutil apfs unlockVolume {apfs_volume_label_with_quotes} -mountpoint {mount_point:?} -stdinpassphrase");
        vec!["/bin/sh".into(), "-c".into(), encrypted_command]
    } else {
        vec![
            "/usr/sbin/diskutil".into(),
            "mount".into(),
            "-mountPoint".into(),
            mount_point.display().to_string(),
            uuid_string,
        ]
    };

    let mount_plist = LaunchctlMountPlist {
        run_at_load: true,
        label: mount_service_label.into(),
        program_arguments: mount_command,
    };

    Ok(mount_plist)
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct LaunchctlMountPlist {
    run_at_load: bool,
    label: String,
    program_arguments: Vec<String>,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum CreateVolumeServiceError {
    #[error("`{path}` contents differs, planned `{expected:?}`, discovered `{discovered:?}`")]
    DifferentPlist {
        expected: LaunchctlMountPlist,
        discovered: LaunchctlMountPlist,
        path: PathBuf,
    },
    #[error("UUID for APFS volume labelled `{0}` was not found")]
    CannotDetermineUuid(String),
    #[error("An APFS volume labelled `{1}` does not exist, but there exists an fstab entry for that volume, as well as a service file at `{0}`. Consider removing the line containing `/nix` from the `/etc/fstab` and running `sudo rm {0}`")]
    VolumeDoesNotExistButVolumeServiceAndFstabEntryDoes(PathBuf, String),
}

impl From<CreateVolumeServiceError> for ActionErrorKind {
    fn from(val: CreateVolumeServiceError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}
