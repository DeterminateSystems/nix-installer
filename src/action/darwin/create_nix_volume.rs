use crate::action::{
    base::{create_or_insert_into_file, CreateFile, CreateOrInsertIntoFile},
    darwin::{
        BootstrapApfsVolume, CreateApfsVolume, CreateSyntheticObjects, EnableOwnership,
        EncryptApfsVolume, UnmountApfsVolume,
    },
    Action, ActionDescription, ActionError, StatefulAction,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use tracing::{span, Span};

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
    create_or_append_fstab: StatefulAction<CreateOrInsertIntoFile>,
    encrypt_volume: Option<StatefulAction<EncryptApfsVolume>>,
    setup_volume_daemon: StatefulAction<CreateFile>,
    bootstrap_volume: StatefulAction<BootstrapApfsVolume>,
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
            0o0655,
            "nix\n".into(), /* The newline is required otherwise it segfaults */
            create_or_insert_into_file::Position::End,
        )
        .await
        .map_err(|e| ActionError::Child(Box::new(e)))?;

        let create_synthetic_objects = CreateSyntheticObjects::plan().await?;

        let unmount_volume = UnmountApfsVolume::plan(disk, name.clone()).await?;

        let create_volume = CreateApfsVolume::plan(disk, name.clone(), case_sensitive).await?;

        let create_or_append_fstab = CreateOrInsertIntoFile::plan(
            "/etc/fstab",
            None,
            None,
            0o0655,
            format!("NAME=\"{name}\" /nix apfs rw,noauto,nobrowse,suid,owners"),
            create_or_insert_into_file::Position::End,
        )
        .await
        .map_err(|e| ActionError::Child(Box::new(e)))?;

        let encrypt_volume = if encrypt {
            Some(EncryptApfsVolume::plan(disk, &name).await?)
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
            CreateFile::plan(NIX_VOLUME_MOUNTD_DEST, None, None, None, mount_plist, false).await?;

        let bootstrap_volume = BootstrapApfsVolume::plan(NIX_VOLUME_MOUNTD_DEST).await?;
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
            setup_volume_daemon,
            bootstrap_volume,
            enable_ownership,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_apfs_volume")]
impl Action for CreateNixVolume {
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create an APFS volume `{}` for Nix on `{}`",
            self.name,
            self.disk.display()
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
        let Self {
            disk: _, name: _, ..
        } = &self;
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
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
            setup_volume_daemon,
            bootstrap_volume,
            enable_ownership,
        } = self;

        create_or_append_synthetic_conf.try_execute().await?;
        create_synthetic_objects.try_execute().await?;
        unmount_volume.try_execute().await.ok(); // We actually expect this may fail.
        create_volume.try_execute().await?;
        create_or_append_fstab.try_execute().await?;
        if let Some(encrypt_volume) = encrypt_volume {
            encrypt_volume.try_execute().await?;
        }
        setup_volume_daemon.try_execute().await?;

        bootstrap_volume.try_execute().await?;

        let mut retry_tokens: usize = 50;
        loop {
            tracing::trace!(%retry_tokens, "Checking for Nix Store existence");
            let status = Command::new("/usr/sbin/diskutil")
                .args(["info", "/nix"])
                .stderr(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .status()
                .await
                .map_err(|e| ActionError::Command(e))?;
            if status.success() || retry_tokens == 0 {
                break;
            } else {
                retry_tokens = retry_tokens.saturating_sub(1);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        enable_ownership.try_execute().await?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self { disk, name, .. } = &self;
        vec![ActionDescription::new(
            format!("Remove the APFS volume `{name}` on `{}`", disk.display()),
            vec![format!(
                "Create a writable, persistent systemd system extension.",
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
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
            setup_volume_daemon,
            bootstrap_volume,
            enable_ownership,
        } = self;

        enable_ownership.try_revert().await?;
        bootstrap_volume.try_revert().await?;
        setup_volume_daemon.try_revert().await?;
        if let Some(encrypt_volume) = encrypt_volume {
            encrypt_volume.try_revert().await?;
        }
        create_or_append_fstab.try_revert().await?;

        unmount_volume.try_revert().await?;
        create_volume.try_revert().await?;

        // Purposefully not reversed
        create_or_append_synthetic_conf.try_revert().await?;
        create_synthetic_objects.try_revert().await?;

        Ok(())
    }
}
