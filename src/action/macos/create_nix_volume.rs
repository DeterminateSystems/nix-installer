use crate::action::{
    base::{create_or_insert_into_file, CreateOrInsertIntoFile},
    macos::{
        BootstrapLaunchctlService, CreateApfsVolume, CreateSyntheticObjects, EnableOwnership,
        EncryptApfsVolume, UnmountApfsVolume,
    },
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use tracing::{span, Span};

use super::{
    create_fstab_entry::CreateFstabEntry, CreateVolumeService, KickstartLaunchctlService,
    DARWIN_LAUNCHD_DOMAIN,
};

pub const NIX_VOLUME_MOUNTD_DEST: &str = "/Library/LaunchDaemons/org.nixos.darwin-store.plist";

/// Create an APFS volume
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_apfs_volume")]
pub struct CreateNixVolume {
    disk: PathBuf,
    name: String,
    case_sensitive: bool,
    encrypt: bool,
    create_or_append_synthetic_conf: StatefulAction<CreateOrInsertIntoFile>,
    create_synthetic_objects: StatefulAction<CreateSyntheticObjects>,
    unmount_volume: StatefulAction<UnmountApfsVolume>,
    create_volume: StatefulAction<CreateApfsVolume>,
    create_fstab_entry: StatefulAction<CreateFstabEntry>,
    encrypt_volume: Option<StatefulAction<EncryptApfsVolume>>,
    setup_volume_daemon: StatefulAction<CreateVolumeService>,
    bootstrap_volume: StatefulAction<BootstrapLaunchctlService>,
    kickstart_launchctl_service: StatefulAction<KickstartLaunchctlService>,
    enable_ownership: StatefulAction<EnableOwnership>,
}

impl CreateNixVolume {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: String,
        case_sensitive: bool,
        encrypt: bool,
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

        let encrypt_volume = if encrypt {
            Some(EncryptApfsVolume::plan(false, disk, &name, &create_volume).await?)
        } else {
            None
        };

        let setup_volume_daemon = CreateVolumeService::plan(
            NIX_VOLUME_MOUNTD_DEST,
            "org.nixos.darwin-store",
            name.clone(),
            "/nix",
            encrypt,
        )
        .await
        .map_err(Self::error)?;

        let bootstrap_volume =
            BootstrapLaunchctlService::plan("org.nixos.darwin-store", NIX_VOLUME_MOUNTD_DEST)
                .await
                .map_err(Self::error)?;
        let kickstart_launchctl_service =
            KickstartLaunchctlService::plan(DARWIN_LAUNCHD_DOMAIN, "org.nixos.darwin-store")
                .await
                .map_err(Self::error)?;
        let enable_ownership = EnableOwnership::plan("/nix").await.map_err(Self::error)?;

        Ok(Self {
            disk: disk.to_path_buf(),
            name,
            case_sensitive,
            encrypt,
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
#[typetag::serde(name = "create_apfs_volume")]
impl Action for CreateNixVolume {
    fn action_tag() -> ActionTag {
        ActionTag("create_nix_volume")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create an{maybe_encrypted} APFS volume `{name}` for Nix on `{disk}` and add it to `/etc/fstab` mounting on `/nix`",
            maybe_encrypted = if self.encrypt { " encrypted" } else { "" },
            name = self.name,
            disk = self.disk.display(),
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "create_apfs_volume",
            disk = tracing::field::display(self.disk.display()),
            name = self.name
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let mut explanation = vec![
            self.create_or_append_synthetic_conf.tracing_synopsis(),
            self.create_synthetic_objects.tracing_synopsis(),
            self.unmount_volume.tracing_synopsis(),
            self.create_volume.tracing_synopsis(),
            self.create_fstab_entry.tracing_synopsis(),
        ];
        if let Some(encrypt_volume) = &self.encrypt_volume {
            explanation.push(encrypt_volume.tracing_synopsis());
        }
        explanation.push(self.setup_volume_daemon.tracing_synopsis());
        explanation.push(self.bootstrap_volume.tracing_synopsis());
        explanation.push(self.enable_ownership.tracing_synopsis());

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
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

        let mut retry_tokens: usize = 50;
        loop {
            let mut command = Command::new("/usr/sbin/diskutil");
            command.args(["info", "-plist"]);
            command.arg(&self.name);
            command.stderr(std::process::Stdio::null());
            command.stdout(std::process::Stdio::null());
            tracing::trace!(%retry_tokens, command = ?command.as_std(), "Checking for Nix Store volume existence");
            let output = command
                .output()
                .await
                .map_err(|e| ActionErrorKind::command(&command, e))
                .map_err(Self::error)?;
            if output.status.success() {
                break;
            } else if retry_tokens == 0 {
                return Err(Self::error(ActionErrorKind::command_output(
                    &command, output,
                )));
            } else {
                retry_tokens = retry_tokens.saturating_sub(1);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        self.create_fstab_entry
            .try_execute()
            .await
            .map_err(Self::error)?;
        if let Some(encrypt_volume) = &mut self.encrypt_volume {
            encrypt_volume.try_execute().await.map_err(Self::error)?
        }
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

        crate::action::macos::wait_for_nix_store_dir()
            .await
            .map_err(Self::error)?;

        self.enable_ownership
            .try_execute()
            .await
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let mut explanation = vec![
            self.create_or_append_synthetic_conf.tracing_synopsis(),
            self.create_synthetic_objects.tracing_synopsis(),
            self.unmount_volume.tracing_synopsis(),
            self.create_volume.tracing_synopsis(),
            self.create_fstab_entry.tracing_synopsis(),
        ];
        if let Some(encrypt_volume) = &self.encrypt_volume {
            explanation.push(encrypt_volume.tracing_synopsis());
        }
        explanation.push(self.setup_volume_daemon.tracing_synopsis());
        explanation.push(self.bootstrap_volume.tracing_synopsis());
        explanation.push(self.enable_ownership.tracing_synopsis());

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

        if let Some(encrypt_volume) = &mut self.encrypt_volume {
            if let Err(err) = encrypt_volume.try_revert().await {
                errors.push(err)
            }
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
