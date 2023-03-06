use crate::action::{
    base::{create_or_insert_into_file, CreateFile, CreateOrInsertIntoFile},
    macos::{
        BootstrapLaunchctlService, CreateApfsVolume, CreateSyntheticObjects, EnableOwnership,
        EncryptApfsVolume, UnmountApfsVolume,
    },
    Action, ActionDescription, ActionError, ActionTag, StatefulAction,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use tracing::{span, Span};

use super::{create_fstab_entry::CreateFstabEntry, KickstartLaunchctlService};

pub const NIX_VOLUME_MOUNTD_DEST: &str = "/Library/LaunchDaemons/org.nixos.darwin-store.plist";

/// Create an APFS volume
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
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
    setup_volume_daemon: StatefulAction<CreateFile>,
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
        .map_err(|e| ActionError::Child(CreateOrInsertIntoFile::action_tag(), Box::new(e)))?;

        let create_synthetic_objects = CreateSyntheticObjects::plan()
            .await
            .map_err(|e| ActionError::Child(CreateSyntheticObjects::action_tag(), Box::new(e)))?;

        let unmount_volume = UnmountApfsVolume::plan(disk, name.clone())
            .await
            .map_err(|e| ActionError::Child(UnmountApfsVolume::action_tag(), Box::new(e)))?;

        let create_volume = CreateApfsVolume::plan(disk, name.clone(), case_sensitive)
            .await
            .map_err(|e| ActionError::Child(CreateApfsVolume::action_tag(), Box::new(e)))?;

        let create_fstab_entry = CreateFstabEntry::plan(name.clone(), &create_volume)
            .await
            .map_err(|e| ActionError::Child(CreateFstabEntry::action_tag(), Box::new(e)))?;

        let encrypt_volume = if encrypt {
            Some(EncryptApfsVolume::plan(disk, &name, &create_volume).await?)
        } else {
            None
        };

        let name_with_qoutes = format!("\"{name}\"");
        let encrypted_command;
        let mount_command = if encrypt {
            encrypted_command = format!("/usr/bin/security find-generic-password -s {name_with_qoutes} -w |  /usr/sbin/diskutil apfs unlockVolume {name_with_qoutes} -mountpoint /nix -stdinpassphrase");
            vec!["/bin/sh", "-c", encrypted_command.as_str()]
        } else {
            vec![
                "/usr/sbin/diskutil",
                "mount",
                "-mountPoint",
                "/nix",
                name.as_str(),
            ]
        };
        // TODO(@hoverbear): Use plist lib we have in tree...
        let mount_plist = format!(
            "\
            <?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
            <!DOCTYPE plist PUBLIC \"-//Apple Computer//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
            <plist version=\"1.0\">\n\
            <dict>\n\
            <key>RunAtLoad</key>\n\
            <true/>\n\
            <key>Label</key>\n\
            <string>org.nixos.darwin-store</string>\n\
            <key>ProgramArguments</key>\n\
            <array>\n\
                {}\
            </array>\n\
            </dict>\n\
            </plist>\n\
        \
        ", mount_command.iter().map(|v| format!("<string>{v}</string>\n")).collect::<Vec<_>>().join("\n")
        );
        let setup_volume_daemon =
            CreateFile::plan(NIX_VOLUME_MOUNTD_DEST, None, None, None, mount_plist, false)
                .await
                .map_err(|e| ActionError::Child(CreateFile::action_tag(), Box::new(e)))?;

        let bootstrap_volume = BootstrapLaunchctlService::plan(
            "system",
            "org.nixos.darwin-store",
            NIX_VOLUME_MOUNTD_DEST,
        )
        .await
        .map_err(|e| ActionError::Child(BootstrapLaunchctlService::action_tag(), Box::new(e)))?;
        let kickstart_launchctl_service =
            KickstartLaunchctlService::plan("system/org.nixos.darwin-store")
                .await
                .map_err(|e| {
                    ActionError::Child(KickstartLaunchctlService::action_tag(), Box::new(e))
                })?;
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
        explanation.append(&mut vec![
            self.setup_volume_daemon.tracing_synopsis(),
            self.bootstrap_volume.tracing_synopsis(),
            self.enable_ownership.tracing_synopsis(),
        ]);

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        self.create_or_append_synthetic_conf
            .try_execute()
            .await
            .map_err(|e| {
                ActionError::Child(
                    self.create_or_append_synthetic_conf.action_tag(),
                    Box::new(e),
                )
            })?;
        self.create_synthetic_objects
            .try_execute()
            .await
            .map_err(|e| {
                ActionError::Child(self.create_synthetic_objects.action_tag(), Box::new(e))
            })?;
        self.unmount_volume.try_execute().await.ok(); // We actually expect this may fail.
        self.create_volume
            .try_execute()
            .await
            .map_err(|e| ActionError::Child(self.create_volume.action_tag(), Box::new(e)))?;
        self.create_fstab_entry
            .try_execute()
            .await
            .map_err(|e| ActionError::Child(self.create_fstab_entry.action_tag(), Box::new(e)))?;
        if let Some(encrypt_volume) = &mut self.encrypt_volume {
            encrypt_volume
                .try_execute()
                .await
                .map_err(|e| ActionError::Child(encrypt_volume.action_tag(), Box::new(e)))?
        }
        self.setup_volume_daemon
            .try_execute()
            .await
            .map_err(|e| ActionError::Child(self.setup_volume_daemon.action_tag(), Box::new(e)))?;

        self.bootstrap_volume
            .try_execute()
            .await
            .map_err(|e| ActionError::Child(self.bootstrap_volume.action_tag(), Box::new(e)))?;
        self.kickstart_launchctl_service
            .try_execute()
            .await
            .map_err(|e| {
                ActionError::Child(self.kickstart_launchctl_service.action_tag(), Box::new(e))
            })?;

        let mut retry_tokens: usize = 50;
        loop {
            tracing::trace!(%retry_tokens, "Checking for Nix Store existence");
            let mut command = Command::new("/usr/sbin/diskutil");
            command.args(["info", "/nix"]);
            command.stderr(std::process::Stdio::null());
            command.stdout(std::process::Stdio::null());
            let status = command
                .status()
                .await
                .map_err(|e| ActionError::command(&command, e))?;
            if status.success() || retry_tokens == 0 {
                break;
            } else {
                retry_tokens = retry_tokens.saturating_sub(1);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        self.enable_ownership
            .try_execute()
            .await
            .map_err(|e| ActionError::Child(self.enable_ownership.action_tag(), Box::new(e)))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self { disk, name, .. } = &self;
        // TODO(@hoverbear): Do a better description here.
        vec![ActionDescription::new(
            format!("Remove the APFS volume `{name}` on `{}`", disk.display()),
            vec![format!(
                "Create a writable, persistent systemd system extension.",
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        self.enable_ownership.try_revert().await?;
        self.kickstart_launchctl_service.try_revert().await?;
        self.bootstrap_volume.try_revert().await?;
        self.setup_volume_daemon.try_revert().await?;
        if let Some(encrypt_volume) = &mut self.encrypt_volume {
            encrypt_volume.try_revert().await?;
        }
        self.create_fstab_entry
            .try_revert()
            .await
            .map_err(|e| ActionError::Child(self.create_fstab_entry.action_tag(), Box::new(e)))?;

        self.unmount_volume
            .try_revert()
            .await
            .map_err(|e| ActionError::Child(self.unmount_volume.action_tag(), Box::new(e)))?;
        self.create_volume
            .try_revert()
            .await
            .map_err(|e| ActionError::Child(self.create_volume.action_tag(), Box::new(e)))?;

        // Purposefully not reversed
        self.create_or_append_synthetic_conf
            .try_revert()
            .await
            .map_err(|e| {
                ActionError::Child(
                    self.create_or_append_synthetic_conf.action_tag(),
                    Box::new(e),
                )
            })?;
        self.create_synthetic_objects
            .try_revert()
            .await
            .map_err(|e| {
                ActionError::Child(self.create_synthetic_objects.action_tag(), Box::new(e))
            })?;

        Ok(())
    }
}
