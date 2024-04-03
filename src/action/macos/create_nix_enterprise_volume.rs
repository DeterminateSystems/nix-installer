use crate::action::{
    base::{create_or_insert_into_file, CreateOrInsertIntoFile},
    macos::{
        CreateApfsVolume, CreateSyntheticObjects, EnableOwnership, EncryptApfsVolume,
        UnmountApfsVolume,
    },
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use tracing::{span, Span};

use super::create_fstab_entry::CreateFstabEntry;

/// Create an APFS volume
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateNixEnterpriseVolume {
    disk: PathBuf,
    name: String,
    case_sensitive: bool,
    create_or_append_synthetic_conf: StatefulAction<CreateOrInsertIntoFile>,
    create_synthetic_objects: StatefulAction<CreateSyntheticObjects>,
    unmount_volume: StatefulAction<UnmountApfsVolume>,
    create_volume: StatefulAction<CreateApfsVolume>,
    create_fstab_entry: StatefulAction<CreateFstabEntry>,
    encrypt_volume: StatefulAction<EncryptApfsVolume>,
    enable_ownership: StatefulAction<EnableOwnership>,
}

impl CreateNixEnterpriseVolume {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        disk: impl AsRef<Path>,
        name: String,
        case_sensitive: bool,
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

        let encrypt_volume = EncryptApfsVolume::plan(true, disk, &name, &create_volume).await?;

        let enable_ownership = EnableOwnership::plan("/nix").await.map_err(Self::error)?;

        Ok(Self {
            disk: disk.to_path_buf(),
            name,
            case_sensitive,
            create_or_append_synthetic_conf,
            create_synthetic_objects,
            unmount_volume,
            create_volume,
            create_fstab_entry,
            encrypt_volume,
            enable_ownership,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_apfs_enterprise_volume")]
impl Action for CreateNixEnterpriseVolume {
    fn action_tag() -> ActionTag {
        ActionTag("create_nix_enterprise_volume")
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
            "create_apfs_volume",
            disk = tracing::field::display(self.disk.display()),
            name = self.name
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let explanation = vec![
            self.create_or_append_synthetic_conf.tracing_synopsis(),
            self.create_synthetic_objects.tracing_synopsis(),
            self.unmount_volume.tracing_synopsis(),
            self.create_volume.tracing_synopsis(),
            self.create_fstab_entry.tracing_synopsis(),
            self.encrypt_volume.tracing_synopsis(),
            self.enable_ownership.tracing_synopsis(),
        ];

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

        self.encrypt_volume
            .try_execute()
            .await
            .map_err(Self::error)?;

        let mut command = Command::new("/usr/local/bin/determinate-nix-for-macos");
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

        let mut retry_tokens: usize = 50;
        loop {
            let mut command = Command::new("/usr/sbin/diskutil");
            command.args(["info", "/nix"]);
            command.stderr(std::process::Stdio::null());
            command.stdout(std::process::Stdio::null());
            tracing::trace!(%retry_tokens, command = ?command.as_std(), "Checking for Nix Store mount path existence");
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

        self.enable_ownership
            .try_execute()
            .await
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let explanation = vec![
            self.create_or_append_synthetic_conf.tracing_synopsis(),
            self.create_synthetic_objects.tracing_synopsis(),
            self.unmount_volume.tracing_synopsis(),
            self.create_volume.tracing_synopsis(),
            self.create_fstab_entry.tracing_synopsis(),
            self.encrypt_volume.tracing_synopsis(),
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
