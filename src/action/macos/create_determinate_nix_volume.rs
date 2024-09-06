use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{span, Span};

use super::{create_fstab_entry::CreateFstabEntry, DARWIN_LAUNCHD_DOMAIN};
use crate::action::macos::{
    BootstrapLaunchctlService, CreateDeterminateVolumeService, KickstartLaunchctlService,
};
use crate::action::{
    base::{create_or_insert_into_file, CreateDirectory, CreateOrInsertIntoFile},
    common::place_nix_configuration::NIX_CONF_FOLDER,
    macos::{
        CreateApfsVolume, CreateSyntheticObjects, EnableOwnership, EncryptApfsVolume,
        UnmountApfsVolume,
    },
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

pub const VOLUME_MOUNT_SERVICE_NAME: &str = "systems.determinate.nix-store";
pub const VOLUME_MOUNT_SERVICE_DEST: &str =
    "/Library/LaunchDaemons/systems.determinate.nix-store.plist";

/// Create an APFS volume
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_determinate_nix_volume")]
pub struct CreateDeterminateNixVolume {
    disk: PathBuf,
    name: String,
    case_sensitive: bool,
    create_directory: StatefulAction<CreateDirectory>,
    create_or_append_synthetic_conf: StatefulAction<CreateOrInsertIntoFile>,
    create_synthetic_objects: StatefulAction<CreateSyntheticObjects>,
    unmount_volume: StatefulAction<UnmountApfsVolume>,
    create_volume: StatefulAction<CreateApfsVolume>,
    create_fstab_entry: StatefulAction<CreateFstabEntry>,
    encrypt_volume: StatefulAction<EncryptApfsVolume>,
    setup_volume_daemon: StatefulAction<CreateDeterminateVolumeService>,
    bootstrap_volume: StatefulAction<BootstrapLaunchctlService>,
    kickstart_launchctl_service: StatefulAction<KickstartLaunchctlService>,
    enable_ownership: StatefulAction<EnableOwnership>,
}

impl CreateDeterminateNixVolume {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: String,
        case_sensitive: bool,
        force: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let disk = disk.as_ref();
        let create_or_append_synthetic_conf = CreateOrInsertIntoFile::plan(
            "/etc/synthetic.conf",
            None,
            None,
            None,
            "nix\n".into(), /* The newline is required otherwise it segfaults */
            create_or_insert_into_file::Position::End,
        )
        .await
        .map_err(Self::error)?;

        let create_directory = CreateDirectory::plan(NIX_CONF_FOLDER, None, None, 0o0755, force)
            .await
            .map_err(Self::error)?;

        let create_synthetic_objects = CreateSyntheticObjects::plan().await.map_err(Self::error)?;

        let unmount_volume = UnmountApfsVolume::plan(disk, name.clone())
            .await
            .map_err(Self::error)?;

        let create_volume = CreateApfsVolume::plan(disk, name.clone(), case_sensitive)
            .await
            .map_err(Self::error)?;

        let create_fstab_entry = CreateFstabEntry::plan(name.clone(), &create_volume)
            .await
            .map_err(Self::error)?;

        let encrypt_volume = EncryptApfsVolume::plan(true, disk, &name, &create_volume).await?;

        let setup_volume_daemon = CreateDeterminateVolumeService::plan(
            VOLUME_MOUNT_SERVICE_DEST,
            VOLUME_MOUNT_SERVICE_NAME,
        )
        .await
        .map_err(Self::error)?;

        let bootstrap_volume =
            BootstrapLaunchctlService::plan(VOLUME_MOUNT_SERVICE_NAME, VOLUME_MOUNT_SERVICE_DEST)
                .await
                .map_err(Self::error)?;
        let kickstart_launchctl_service =
            KickstartLaunchctlService::plan(DARWIN_LAUNCHD_DOMAIN, VOLUME_MOUNT_SERVICE_NAME)
                .await
                .map_err(Self::error)?;

        let enable_ownership = EnableOwnership::plan("/nix").await.map_err(Self::error)?;

        Ok(Self {
            disk: disk.to_path_buf(),
            name,
            case_sensitive,
            create_directory,
            create_or_append_synthetic_conf,
            create_synthetic_objects,
            unmount_volume,
            create_volume,
            create_fstab_entry,
            encrypt_volume,
            setup_volume_daemon,
            bootstrap_volume,
            kickstart_launchctl_service,
            enable_ownership,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_determinate_nix_volume")]
impl Action for CreateDeterminateNixVolume {
    fn action_tag() -> ActionTag {
        ActionTag("create_determinate_nix_volume")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create an encrypted APFS volume `{name}` for Nix on `{disk}` and add it to `/etc/fstab` mounting on `/nix`",
            name = self.name,
            disk = self.disk.display(),
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "create_determinate_nix_volume",
            disk = tracing::field::display(self.disk.display()),
            name = self.name
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let explanation = vec![
            self.create_directory.tracing_synopsis(),
            self.create_or_append_synthetic_conf.tracing_synopsis(),
            self.create_synthetic_objects.tracing_synopsis(),
            self.unmount_volume.tracing_synopsis(),
            self.create_volume.tracing_synopsis(),
            self.create_fstab_entry.tracing_synopsis(),
            self.encrypt_volume.tracing_synopsis(),
            self.setup_volume_daemon.tracing_synopsis(),
            self.bootstrap_volume.tracing_synopsis(),
            self.kickstart_launchctl_service.tracing_synopsis(),
            self.enable_ownership.tracing_synopsis(),
        ];

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        self.create_directory
            .try_execute()
            .await
            .map_err(Self::error)?;
        self.create_or_append_synthetic_conf
            .try_execute()
            .await
            .map_err(Self::error)?;
        self.create_synthetic_objects
            .try_execute()
            .await
            .map_err(Self::error)?;
        self.unmount_volume.try_execute().await.ok(); // We actually expect this may fail.
        self.create_volume
            .try_execute()
            .await
            .map_err(Self::error)?;

        crate::action::macos::wait_for_nix_store_dir()
            .await
            .map_err(Self::error)?;

        self.create_fstab_entry
            .try_execute()
            .await
            .map_err(Self::error)?;

        self.encrypt_volume
            .try_execute()
            .await
            .map_err(Self::error)?;

        let mut command = Command::new("/usr/local/bin/determinate-nixd");
        command.args(["--stop-after", "mount"]);
        command.stderr(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        tracing::trace!(command = ?command.as_std(), "Mounting /nix");
        let output = command
            .output()
            .await
            .map_err(|e| ActionErrorKind::command(&command, e))
            .map_err(Self::error)?;
        if !output.status.success() {
            return Err(Self::error(ActionErrorKind::command_output(
                &command, output,
            )));
        }

        crate::action::macos::wait_for_nix_store_dir()
            .await
            .map_err(Self::error)?;

        self.setup_volume_daemon
            .try_execute()
            .await
            .map_err(Self::error)?;

        self.bootstrap_volume
            .try_execute()
            .await
            .map_err(Self::error)?;

        self.kickstart_launchctl_service
            .try_execute()
            .await
            .map_err(Self::error)?;

        self.enable_ownership
            .try_execute()
            .await
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let explanation = vec![
            self.create_directory.tracing_synopsis(),
            self.create_or_append_synthetic_conf.tracing_synopsis(),
            self.create_synthetic_objects.tracing_synopsis(),
            self.unmount_volume.tracing_synopsis(),
            self.create_volume.tracing_synopsis(),
            self.create_fstab_entry.tracing_synopsis(),
            self.encrypt_volume.tracing_synopsis(),
            self.setup_volume_daemon.tracing_synopsis(),
            self.bootstrap_volume.tracing_synopsis(),
            self.kickstart_launchctl_service.tracing_synopsis(),
            self.enable_ownership.tracing_synopsis(),
        ];

        vec![ActionDescription::new(
            format!(
                "Remove the APFS volume `{}` on `{}`",
                self.name,
                self.disk.display()
            ),
            explanation,
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];

        if let Err(err) = self.enable_ownership.try_revert().await {
            errors.push(err)
        };
        if let Err(err) = self.kickstart_launchctl_service.try_revert().await {
            errors.push(err)
        }
        if let Err(err) = self.bootstrap_volume.try_revert().await {
            errors.push(err)
        }
        if let Err(err) = self.setup_volume_daemon.try_revert().await {
            errors.push(err)
        }
        if let Err(err) = self.encrypt_volume.try_revert().await {
            errors.push(err)
        }
        if let Err(err) = self.create_fstab_entry.try_revert().await {
            errors.push(err)
        }

        if let Err(err) = self.unmount_volume.try_revert().await {
            errors.push(err)
        }
        if let Err(err) = self.create_volume.try_revert().await {
            errors.push(err)
        }

        // Purposefully not reversed
        if let Err(err) = self.create_or_append_synthetic_conf.try_revert().await {
            errors.push(err)
        }
        if let Err(err) = self.create_synthetic_objects.try_revert().await {
            errors.push(err)
        }

        if let Err(err) = self.create_directory.try_revert().await {
            errors.push(err);
        }

        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(errors
                .into_iter()
                .next()
                .expect("Expected 1 len Vec to have at least 1 item"))
        } else {
            Err(Self::error(ActionErrorKind::MultipleChildren(errors)))
        }
    }
}
