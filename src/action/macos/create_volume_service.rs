use serde::{Deserialize, Serialize};
use tracing::{span, Span};

use std::path::{Path, PathBuf};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::AsyncWriteExt,
};

use crate::action::{Action, ActionDescription, ActionError, ActionTag, StatefulAction};

use super::get_uuid_for_label;

/** Create a plist for a `launchctl` service to mount the given `apfs_volume_label` on the given `mount_point`.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateVolumeService {
    pub(crate) path: PathBuf,
    apfs_volume_label: String,
    mount_service_label: String,
    mount_point: PathBuf,
    encrypt: bool,
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
        let this = Self {
            path,
            apfs_volume_label,
            mount_service_label,
            mount_point,
            encrypt,
        };

        if this.path.exists() {
            let discovered_plist: LaunchctlMountPlist = plist::from_file(&this.path)?;
            let expected_plist = generate_mount_plist(
                &this.mount_service_label,
                &this.apfs_volume_label,
                &this.mount_point,
                encrypt,
            )
            .await?;
            if discovered_plist != expected_plist {
                tracing::trace!(
                    ?discovered_plist,
                    ?expected_plist,
                    "Parsed plists not equal"
                );
                return Err(ActionError::Custom(Box::new(
                    CreateVolumeServiceError::DifferentPlist {
                        expected: expected_plist,
                        discovered: discovered_plist,
                        path: this.path.clone(),
                    },
                )));
            }

            tracing::debug!("Creating file `{}` already complete", this.path.display());
            return Ok(StatefulAction::completed(this));
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_volume_service")]
impl Action for CreateVolumeService {
    fn action_tag() -> ActionTag {
        ActionTag("setup_volume_daemon")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Create a `launchctl` plist to mount the APFS volume `{}`",
            self.path.display()
        )
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "setup_volume_daemon",
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
        } = self;

        let generated_plist = generate_mount_plist(
            &mount_service_label,
            &apfs_volume_label,
            mount_point,
            *encrypt,
        )
        .await?;

        let mut options = OpenOptions::new();
        options.create_new(true).write(true).read(true);

        let mut file = options
            .open(&path)
            .await
            .map_err(|e| ActionError::Open(path.to_owned(), e))?;

        let mut buf = Vec::new();
        plist::to_writer_xml(&mut buf, &generated_plist)?;
        file.write_all(&buf)
            .await
            .map_err(|e| ActionError::Write(path.to_owned(), e))?;

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
            .map_err(|e| ActionError::Remove(self.path.to_owned(), e))?;

        Ok(())
    }
}

/// This function must be able to operate at both plan and execute time.
async fn generate_mount_plist(
    mount_service_label: &str,
    apfs_volume_label: &str,
    mount_point: &Path,
    encrypt: bool,
) -> Result<LaunchctlMountPlist, ActionError> {
    let apfs_volume_label_with_qoutes = format!("\"{apfs_volume_label}\"");
    let uuid = get_uuid_for_label(&apfs_volume_label).await?;
    // The official Nix scripts uppercase the UUID, so we do as well for compatability.
    let uuid_string = uuid.to_string().to_uppercase();
    let encrypted_command;
    let mount_command = if encrypt {
        encrypted_command = format!("/usr/bin/security find-generic-password -s {apfs_volume_label_with_qoutes} -w |  /usr/sbin/diskutil apfs unlockVolume {apfs_volume_label_with_qoutes} -mountpoint {mount_point:?} -stdinpassphrase");
        vec!["/bin/sh".into(), "-c".into(), encrypted_command]
    } else {
        vec![
            "/usr/sbin/diskutil".into(),
            "mount".into(),
            "-mountPoint".into(),
            format!("{mount_point:?}"), // Debug for escaped path
            uuid_string,
        ]
    };
    // let mount_plist = format!(
    //     "\
    //     <?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
    //     <!DOCTYPE plist PUBLIC \"-//Apple Computer//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
    //     <plist version=\"1.0\">\n\
    //     <dict>\n\
    //     {two_spaces}<key>RunAtLoad</key>\n\
    //     {two_spaces}<true/>\n\
    //     {two_spaces}<key>Label</key>\n\
    //     <string>org.nixos.darwin-store</string>\n\
    //     {two_spaces}<key>ProgramArguments</key>\n\
    //     {two_spaces}<array>\n\
    //     {two_spaces}  {}\
    //     {two_spaces}</array>\n\
    //     </dict>\n\
    //     </plist>\n\
    // \
    // ",
    //     mount_command.iter().map(|v| format!("  <string>{v}</string>")).collect::<Vec<_>>().join("\n"),
    //     two_spaces = "  ",
    // );

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
}
