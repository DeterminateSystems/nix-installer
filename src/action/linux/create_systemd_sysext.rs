use tokio::process::Command;

use crate::action::base::{CreateDirectory, CreateDirectoryError, CreateFile, CreateFileError};
use crate::execute_command;
use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};
use std::path::{Path, PathBuf};

const PATHS: &[&str] = &[
    "usr",
    "usr/lib",
    "usr/lib/extension-release.d",
    "usr/lib/systemd",
    "usr/lib/systemd/system",
    "usr/lib/systemd/system/multi-user.target.wants",
];

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateSystemdSysext {
    destination: PathBuf,
    persistence: PathBuf,
    create_directories: Vec<CreateDirectory>,
    create_extension_release: CreateFile,
    nix_directory_unit: CreateFile,
    create_bind_mount_unit: CreateFile,
    action_state: ActionState,
}

impl CreateSystemdSysext {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        destination: impl AsRef<Path>,
        persistence: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let destination = destination.as_ref();
        let persistence = persistence.as_ref();

        let mut create_directories =
            vec![CreateDirectory::plan(destination, None, None, 0o0755, true).await?];
        for path in PATHS {
            create_directories.push(
                CreateDirectory::plan(destination.join(path), None, None, 0o0755, false).await?,
            )
        }

        let version_id = tokio::fs::read_to_string("/etc/os-release")
            .await
            .map(|buf| {
                buf.lines()
                    .find_map(|line| match line.starts_with("VERSION_ID") {
                        true => line.rsplit("=").next().map(|inner| inner.to_owned()),
                        false => None,
                    })
            })
            .map_err(|e| CreateSystemdSysextError::ReadingOsRelease(e).boxed())?
            .ok_or_else(|| CreateSystemdSysextError::NoVersionId.boxed())?;
        let extension_release_buf =
            format!("SYSEXT_LEVEL=1.0\nID=steamos\nVERSION_ID={version_id}");
        let create_extension_release = CreateFile::plan(
            destination.join("usr/lib/extension-release.d/extension-release.nix"),
            None,
            None,
            0o0755,
            extension_release_buf,
            false,
        )
        .await?;

        let nix_directory_buf = format!(
            "
            [Unit]\n\
            Description=Create a `/nix` directory to be used for bind mounting\n\
            \n\
            [Service]\n\
            Type=oneshot\n\
            ExecCondition=sh -c \"if [ -d /nix ]; then exit 1; else exit 0; fi\"
            ExecStart=steamos-readonly disable\n\
            ExecStart=mkdir /nix\n\
            ExecStart=steamos-readonly enable\n\
            ExecStop=steamos-readonly disable\n\
            ExecStop=rmdir /nix\n\
            ExecStop=steamos-readonly enable\n\
            RemainAfterExit=true\n\
        "
        );
        let nix_directory_unit = CreateFile::plan(
            destination.join("usr/lib/systemd/system/nix-directory.service"),
            None,
            None,
            0o0755,
            nix_directory_buf,
            false,
        )
        .await?;

        let create_bind_mount_buf = format!(
            "
            [Unit]\n\
            Description=Mount `{persistence}` on `/nix`\n\
            PropagatesStopTo=nix-daemon.service\n\
            After=nix-directory.service\n\
            Requires=nix-directory.service\n\
            ConditionPathIsDirectory=/nix\n\
            \n\
            [Install]
            RequiredBy=nix-daemon.service\n\
            RequiredBy=nix-daemon.socket\n\
            \n\
            [Mount]\n\
            What={persistence}\n\
            Where=/nix\n\
            Type=none\n\
            Options=bind\n\
        ",
            persistence = persistence.display(),
        );
        let create_bind_mount_unit = CreateFile::plan(
            destination.join("usr/lib/systemd/system/nix.mount"),
            None,
            None,
            0o0755,
            create_bind_mount_buf,
            false,
        )
        .await?;

        Ok(Self {
            destination: destination.to_path_buf(),
            persistence: persistence.to_path_buf(),
            create_directories,
            create_extension_release,
            nix_directory_unit,
            create_bind_mount_unit,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_systemd_sysext")]
impl Action for CreateSystemdSysext {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            action_state: _,
            destination,
            persistence: _,
            create_bind_mount_unit: _,
            nix_directory_unit: _,
            create_directories: _,
            create_extension_release: _,
        } = &self;
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Create a systemd sysext at `{}`", destination.display()),
                vec![format!(
                    "Create a writable, persistent systemd system extension.",
                )],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(destination,))]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            destination: _,
            persistence: _,
            action_state,
            create_directories,
            create_extension_release,
            nix_directory_unit,
            create_bind_mount_unit,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Creating sysext");
            return Ok(());
        }
        tracing::debug!("Creating sysext");

        for create_directory in create_directories {
            create_directory.execute().await?;
        }
        create_extension_release.execute().await?;
        nix_directory_unit.execute().await?;
        create_bind_mount_unit.execute().await?;

        tracing::trace!("Created sysext");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        let Self {
            destination,
            persistence: _,
            action_state: _,
            create_directories: _,
            create_extension_release: _,
            nix_directory_unit: _,
            create_bind_mount_unit: _,
        } = &self;
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                format!("Remove the sysext located at `{}`", destination.display()),
                vec![],
            )]
        }
    }

    #[tracing::instrument(skip_all, fields(destination,))]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {
            destination: _,
            persistence: _,
            action_state,
            create_directories,
            create_extension_release,
            nix_directory_unit,
            create_bind_mount_unit,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Removing sysext");
            return Ok(());
        }
        tracing::debug!("Removing sysext");

        nix_directory_unit.revert().await?;
        create_bind_mount_unit.revert().await?;

        create_extension_release.revert().await?;

        for create_directory in create_directories.iter_mut().rev() {
            create_directory.revert().await?;
        }

        execute_command(Command::new("systemd-sysext").arg("refresh"))
            .await
            .map_err(|e| CreateSystemdSysextError::Command(e).boxed())?;
        execute_command(Command::new("systemctl").arg("daemon-reload"))
            .await
            .map_err(|e| CreateSystemdSysextError::Command(e).boxed())?;

        tracing::trace!("Removed sysext");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }

    fn action_state(&self) -> ActionState {
        self.action_state
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateSystemdSysextError {
    #[error("Command failed to execute")]
    Command(#[source] std::io::Error),
    #[error(transparent)]
    CreateDirectory(#[from] CreateDirectoryError),
    #[error(transparent)]
    CreateFile(#[from] CreateFileError),
    #[error("Reading /etc/os-release")]
    ReadingOsRelease(
        #[source]
        #[from]
        std::io::Error,
    ),
    #[error("No `VERSION_ID` line in /etc/os-release")]
    NoVersionId,
}
