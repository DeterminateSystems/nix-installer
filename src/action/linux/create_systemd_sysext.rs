use std::path::{Path, PathBuf};

use crate::action::common::{CreateDirectory, CreateDirectoryError, CreateFile, CreateFileError};
use crate::{
    action::{Action, ActionDescription, ActionState},
    BoxableError,
};

const PATHS: &[&str] = &[
    "usr",
    "usr/lib",
    "usr/lib/extension-release.d",
    "usr/lib/systemd",
    "usr/lib/systemd/system",
];

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateSystemdSysext {
    destination: PathBuf,
    create_directories: Vec<CreateDirectory>,
    create_extension_release: CreateFile,
    create_bind_mount_unit: CreateFile,
    action_state: ActionState,
}

impl CreateSystemdSysext {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        destination: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let destination = destination.as_ref();

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

        let create_bind_mount_buf = format!(
            "
            [Mount]\n\
            What={}\n\
            Where=/nix\n\
            Type=none\n\
            Options=bind\n\
        ",
            destination.display(),
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
            create_directories,
            create_extension_release,
            create_bind_mount_unit,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create-systemd-sysext")]
impl Action for CreateSystemdSysext {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            action_state: _,
            destination,
            create_bind_mount_unit: _,
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
            action_state,
            create_directories,
            create_extension_release,
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
        create_bind_mount_unit.execute().await?;

        tracing::trace!("Created sysext");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        let Self {
            destination,
            action_state: _,
            create_directories: _,
            create_extension_release: _,
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
            action_state,
            create_directories,
            create_extension_release,
            create_bind_mount_unit,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Removing sysext");
            return Ok(());
        }
        tracing::debug!("Removing sysext");

        create_bind_mount_unit.revert().await?;

        create_extension_release.revert().await?;

        for create_directory in create_directories.iter_mut().rev() {
            create_directory.revert().await?;
        }

        tracing::trace!("Removed sysext");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateSystemdSysextError {
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
