use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::actions::base::{
    darwin::{
        BootstrapVolume, BootstrapVolumeError, CreateSyntheticObjects, CreateSyntheticObjectsError,
        CreateVolume, CreateVolumeError, EnableOwnership, EnableOwnershipError, EncryptVolume,
        EncryptVolumeError, UnmountVolume, UnmountVolumeError,
    },
    CreateDirectory, CreateDirectoryError, CreateFile, CreateFileError, CreateOrAppendFile,
    CreateOrAppendFileError,
};
use crate::actions::{base::darwin, Action, ActionDescription, ActionState, Actionable};

const NIX_VOLUME_MOUNTD_DEST: &str = "/Library/LaunchDaemons/org.nixos.darwin-store.plist";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateApfsVolume {
    disk: PathBuf,
    name: String,
    case_sensitive: bool,
    encrypt: Option<String>,
    create_or_append_synthetic_conf: CreateOrAppendFile,
    create_synthetic_objects: CreateSyntheticObjects,
    unmount_volume: UnmountVolume,
    create_volume: CreateVolume,
    create_or_append_fstab: CreateOrAppendFile,
    encrypt_volume: Option<EncryptVolume>,
    bootstrap_volume: BootstrapVolume,
    enable_ownership: EnableOwnership,
    action_state: ActionState,
}

impl CreateApfsVolume {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: String,
        case_sensitive: bool,
        encrypt: Option<String>,
    ) -> Result<Self, CreateApfsVolumeError> {
        let disk = disk.as_ref();
        let create_or_append_synthetic_conf = CreateOrAppendFile::plan(
            "/etc/synthetic.conf",
            "root".into(),
            "wheel".into(),
            0o0655,
            "nix".into(),
        )
        .await?;

        let create_synthetic_objects = CreateSyntheticObjects::plan().await?;

        let unmount_volume =
            UnmountVolume::plan(disk, name.clone(), case_sensitive, encrypt).await?;

        let create_volume = CreateVolume::plan(disk, name.clone(), case_sensitive, encrypt).await?;

        let create_or_append_fstab = CreateOrAppendFile::plan(
            "/etc/fstab",
            "root".into(),
            "root".into(),
            0o0655,
            "NAME={name} /nix apfs rw,noauto,nobrowse,suid,owners".into(),
        )
        .await?;

        let encrypt_volume = if let Some(password) = encrypt {
            Some(EncryptVolume::plan(disk, password).await)
        } else {
            None
        };

        let bootstrap_volume = BootstrapVolume::plan(NIX_VOLUME_MOUNTD_DEST, disk, name).await?;
        let enable_ownership = EnableOwnership::plan("/nix").await?;

        Ok(Self {
            disk: disk.to_path_buf(),
            name,
            case_sensitive,
            encrypt,
            create_or_append_synthetic_conf,
            create_synthetic_objects,
            unmount_volume,
            create_volume,
            create_or_append_fstab,
            encrypt_volume,
            bootstrap_volume,
            enable_ownership,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for CreateApfsVolume {
    type Error = CreateApfsVolumeError;

    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            disk,
            name,
            case_sensitive,
            encrypt,
            create_or_append_synthetic_conf,
            create_synthetic_objects,
            unmount_volume,
            create_volume,
            create_or_append_fstab,
            encrypt_volume,
            bootstrap_volume,
            enable_ownership,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Create an APFS volume `{name}` on `{}`", disk.display()),
                vec![format!(
                    "Create a writable, persistent systemd system extension.",
                )],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(destination,))]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            disk,
            name,
            case_sensitive,
            encrypt,
            create_or_append_synthetic_conf,
            create_synthetic_objects,
            unmount_volume,
            create_volume,
            create_or_append_fstab,
            encrypt_volume,
            bootstrap_volume,
            enable_ownership,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating APFS volume");
            return Ok(());
        }
        tracing::debug!("Creating APFS volume");

        create_or_append_synthetic_conf.execute().await?;
        create_synthetic_objects.execute().await?;
        unmount_volume.execute().await?;
        create_volume.execute().await?;
        create_or_append_fstab.execute().await?;
        encrypt_volume.execute().await?;
        bootstrap_volume.execute().await?;
        enable_ownership.execute().await?;

        tracing::trace!("Created APFS volume");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        let Self {
            disk,
            name,
            case_sensitive,
            encrypt,
            create_or_append_synthetic_conf,
            create_synthetic_objects,
            unmount_volume,
            create_volume,
            create_or_append_fstab,
            encrypt_volume,
            bootstrap_volume,
            enable_ownership,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Remove the APFS volume `{name}` on `{}`", disk.display()),
                vec![format!(
                    "Create a writable, persistent systemd system extension.",
                )],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(disk, name))]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            disk: _,
            name: _,
            case_sensitive: _,
            encrypt: _,
            create_or_append_synthetic_conf,
            create_synthetic_objects,
            unmount_volume,
            create_volume,
            create_or_append_fstab,
            encrypt_volume,
            bootstrap_volume,
            enable_ownership,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Removing APFS volume");
            return Ok(());
        }
        tracing::debug!("Removing APFS volume");

        enable_ownership.revert().await?;
        bootstrap_volume.revert().await?;
        if let Some(encrypt_volume) = encrypt_volume {
            encrypt_volume.revert().await?;
        }
        create_or_append_fstab.revert().await?;

        unmount_volume.revert().await?;
        create_volume.revert().await?;

        // Purposefully not reversed
        create_or_append_synthetic_conf.revert().await?;
        create_synthetic_objects.revert().await?;

        tracing::trace!("Removed APFS volume");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }
}

impl From<CreateApfsVolume> for Action {
    fn from(v: CreateApfsVolume) -> Self {
        Action::CreateApfsVolume(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateApfsVolumeError {
    #[error(transparent)]
    DarwinBootstrapVolume(#[from] BootstrapVolumeError),
    #[error(transparent)]
    DarwinCreateSyntheticObjects(#[from] CreateSyntheticObjectsError),
    #[error(transparent)]
    DarwinCreateVolume(#[from] CreateVolumeError),
    #[error(transparent)]
    DarwinEnableOwnership(#[from] EnableOwnershipError),
    #[error(transparent)]
    DarwinEncryptVolume(#[from] EncryptVolumeError),
    #[error(transparent)]
    DarwinUnmountVolume(#[from] UnmountVolumeError),
    #[error(transparent)]
    CreateOrAppendFile(#[from] CreateOrAppendFileError),
}
