use uuid::Uuid;

use crate::{
    action::{Action, ActionDescription, ActionError, StatefulAction},
    execute_command,
};
use serde::Deserialize;
use std::{io::SeekFrom, path::Path};
use tokio::{
    fs::OpenOptions,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::Command,
};
use tracing::{span, Span};

const FSTAB_PATH: &str = "/etc/fstab";

/** Create an `/etc/fstab` entry for the given volume


This action queries `diskutil info` on the volume to fetch it's UUID and
add the relevant information to `/etc/fstab`.
 */
// Initially, a `NAME` was used, however in https://github.com/DeterminateSystems/nix-installer/issues/212
// several users reported issues. Using a UUID resolved the issue for them.
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateFstabEntry {
    apfs_volume_label: String,
}

impl CreateFstabEntry {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(apfs_volume_label: String) -> Result<StatefulAction<Self>, ActionError> {
        let fstab_path = Path::new(FSTAB_PATH);
        if fstab_path.exists() {
            let fstab_buf = tokio::fs::read_to_string(&fstab_path)
                .await
                .map_err(|e| ActionError::Read(fstab_path.to_path_buf(), e))?;
            let prelude_comment = fstab_prelude_comment(&apfs_volume_label);

            // See if the user already has a `/nix` related entry, if so, invite them to remove it.
            if fstab_buf.split(&[' ', '\t']).any(|chunk| chunk == "/nix") {
                return Err(ActionError::Custom(Box::new(
                    CreateFstabEntryError::NixEntryExists,
                )));
            }

            // See if a previous install from this crate exists, if so, invite the user to remove it (we may need to change it)
            if fstab_buf.contains(&prelude_comment) {
                return Err(ActionError::Custom(Box::new(
                    CreateFstabEntryError::VolumeEntryExists(apfs_volume_label.clone()),
                )));
            }
        }

        Ok(Self { apfs_volume_label }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_fstab_entry")]
impl Action for CreateFstabEntry {
    fn tracing_synopsis(&self) -> String {
        format!(
            "Add a UUID based entry for the APFS volume `{}` to `/etc/fstab`",
            self.apfs_volume_label
        )
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_fstab_entry",
            apfs_volume_label = self.apfs_volume_label,
        );

        span
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { apfs_volume_label } = self;
        let fstab_path = Path::new(FSTAB_PATH);
        let uuid = get_uuid_for_label(&apfs_volume_label).await?;
        let fstab_entry = fstab_entry(&uuid, apfs_volume_label);

        let mut fstab = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(fstab_path)
            .await
            .map_err(|e| ActionError::Open(fstab_path.to_path_buf(), e))?;

        // Make sure it doesn't already exist before we write to it.
        let mut fstab_buf = String::new();
        fstab
            .read_to_string(&mut fstab_buf)
            .await
            .map_err(|e| ActionError::Read(fstab_path.to_owned(), e))?;

        if fstab_buf.contains(&fstab_entry) {
            tracing::debug!("Skipped writing to `/etc/fstab` as the content already existed")
        } else {
            fstab
                .write_all(fstab_entry.as_bytes())
                .await
                .map_err(|e| ActionError::Write(fstab_path.to_owned(), e))?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self { apfs_volume_label } = &self;
        vec![ActionDescription::new(
            format!(
                "Remove the UUID based entry for the APFS volume `{}` in `/etc/fstab`",
                apfs_volume_label
            ),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self { apfs_volume_label } = self;
        let fstab_path = Path::new(FSTAB_PATH);
        let uuid = get_uuid_for_label(&apfs_volume_label).await?;
        let fstab_entry = fstab_entry(&uuid, apfs_volume_label);

        let mut file = OpenOptions::new()
            .create(false)
            .write(true)
            .read(true)
            .open(&fstab_path)
            .await
            .map_err(|e| ActionError::Open(fstab_path.to_owned(), e))?;

        let mut file_contents = String::default();
        file.read_to_string(&mut file_contents)
            .await
            .map_err(|e| ActionError::Read(fstab_path.to_owned(), e))?;

        if let Some(start) = file_contents.rfind(fstab_entry.as_str()) {
            let end = start + fstab_entry.len();
            file_contents.replace_range(start..end, "")
        }

        file.seek(SeekFrom::Start(0))
            .await
            .map_err(|e| ActionError::Seek(fstab_path.to_owned(), e))?;
        file.set_len(0)
            .await
            .map_err(|e| ActionError::Truncate(fstab_path.to_owned(), e))?;
        file.write_all(file_contents.as_bytes())
            .await
            .map_err(|e| ActionError::Write(fstab_path.to_owned(), e))?;
        file.flush()
            .await
            .map_err(|e| ActionError::Flush(fstab_path.to_owned(), e))?;

        Ok(())
    }
}

async fn get_uuid_for_label(apfs_volume_label: &str) -> Result<Uuid, ActionError> {
    let output = execute_command(
        Command::new("/usr/sbin/diskutil")
            .process_group(0)
            .arg("info")
            .arg("-plist")
            .arg(apfs_volume_label)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped()),
    )
    .await
    .map_err(|e| ActionError::Command(e))?;

    let parsed: DiskUtilApfsInfoOutput = plist::from_bytes(&output.stdout)?;

    Ok(parsed.volume_uuid)
}

fn fstab_prelude_comment(apfs_volume_label: &str) -> String {
    format!("# nix-installer created volume labelled `{apfs_volume_label}`")
}

fn fstab_entry(uuid: &Uuid, apfs_volume_label: &str) -> String {
    let prelude_comment = fstab_prelude_comment(apfs_volume_label);
    format!(
        "\
        {prelude_comment}\n\
        UUID={uuid} /nix apfs rw,noauto,nobrowse,suid,owners\n\
        "
    )
}

#[derive(thiserror::Error, Debug)]
pub enum CreateFstabEntryError {
    #[error("An `/etc/fstab` entry for the `/nix` path already exists, consider removing the entry for `/nix`d from `/etc/fstab`")]
    NixEntryExists,
    #[error("An `/etc/fstab` entry created by `nix-installer` already exists. If a volume named `{0}` already exists, it may need to be deleted with `diskutil apfs deleteVolume \"{0}\" and the entry for `/nix` should be removed from `/etc/fstab`")]
    VolumeEntryExists(String),
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
struct DiskUtilApfsInfoOutput {
    #[serde(rename = "VolumeUUID")]
    volume_uuid: Uuid,
}
