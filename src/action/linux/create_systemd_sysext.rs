use crate::action::base::{CreateDirectory, CreateDirectoryError, CreateFile, CreateFileError};
use crate::action::{ActionError, StatefulAction};
use crate::{
    action::{Action, ActionDescription},
    BoxableError,
};
use std::path::{Path, PathBuf};

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
    create_directories: Vec<StatefulAction<CreateDirectory>>,
    create_extension_release: StatefulAction<CreateFile>,
    create_bind_mount_unit: StatefulAction<CreateFile>,
}

impl CreateSystemdSysext {
    #[tracing::instrument(skip_all)]
    pub async fn plan(destination: impl AsRef<Path>) -> Result<StatefulAction<Self>, ActionError> {
        let destination = destination.as_ref();

        let mut create_directories =
            vec![CreateDirectory::plan(destination, None, None, 0o0755, true)
                .await
                .map_err(|e| ActionError::Child(Box::new(e)))?];
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
        .await
        .map_err(|e| ActionError::Child(Box::new(e)))?;

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
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_systemd_sysext")]
impl Action for CreateSystemdSysext {
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create a systemd sysext at `{}`",
            self.destination.display()
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!(
                "Create a writable, persistent systemd system extension.",
            )],
        )]
    }

    #[tracing::instrument(skip_all, fields(destination,))]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            destination: _,
            create_directories,
            create_extension_release,
            create_bind_mount_unit,
        } = self;

        for create_directory in create_directories {
            create_directory.try_execute().await?;
        }
        create_extension_release.try_execute().await?;
        create_bind_mount_unit.try_execute().await?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!(
                "Remove the sysext located at `{}`",
                self.destination.display()
            ),
            vec![],
        )]
    }

    #[tracing::instrument(skip_all, fields(destination,))]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            destination: _,
            create_directories,
            create_extension_release,
            create_bind_mount_unit,
        } = self;

        create_bind_mount_unit.try_revert().await?;

        create_extension_release.try_revert().await?;

        for create_directory in create_directories.iter_mut().rev() {
            create_directory.try_revert().await?;
        }

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
