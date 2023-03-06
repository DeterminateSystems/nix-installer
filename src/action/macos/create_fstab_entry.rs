use uuid::Uuid;

use super::CreateApfsVolume;
use crate::{
    action::{Action, ActionDescription, ActionError, ActionState, ActionTag, StatefulAction},
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

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Copy)]
enum ExistingFstabEntry {
    /// Need to update the existing `nix-installer` made entry
    NixInstallerEntry,
    /// Need to remove old entry and add new entry
    Foreign,
    None,
}

/** Create an `/etc/fstab` entry for the given volume

This action queries `diskutil info` on the volume to fetch it's UUID and
add the relevant information to `/etc/fstab`.
 */
// Initially, a `NAME` was used, however in https://github.com/DeterminateSystems/nix-installer/issues/212
// several users reported issues. Using a UUID resolved the issue for them.
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateFstabEntry {
    apfs_volume_label: String,
    existing_entry: ExistingFstabEntry,
}

impl CreateFstabEntry {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        apfs_volume_label: String,
        planned_create_apfs_volume: &StatefulAction<CreateApfsVolume>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let fstab_path = Path::new(FSTAB_PATH);

        if fstab_path.exists() {
            let fstab_buf = tokio::fs::read_to_string(&fstab_path)
                .await
                .map_err(|e| ActionError::Read(fstab_path.to_path_buf(), e))?;
            let prelude_comment = fstab_prelude_comment(&apfs_volume_label);

            // See if a previous install from this crate exists, if so, invite the user to remove it (we may need to change it)
            if fstab_buf.contains(&prelude_comment) {
                if planned_create_apfs_volume.state != ActionState::Completed {
                    return Ok(StatefulAction::completed(Self {
                        apfs_volume_label,
                        existing_entry: ExistingFstabEntry::NixInstallerEntry,
                    }));
                }

                return Ok(StatefulAction::uncompleted(Self {
                    apfs_volume_label,
                    existing_entry: ExistingFstabEntry::NixInstallerEntry,
                }));
            } else if fstab_buf
                .lines()
                .any(|line| line.split(&[' ', '\t']).nth(2) == Some("/nix"))
            {
                // See if the user already has a `/nix` related entry, if so, invite them to remove it.
                return Ok(StatefulAction::uncompleted(Self {
                    apfs_volume_label,
                    existing_entry: ExistingFstabEntry::Foreign,
                }));
            }
        }

        Ok(StatefulAction::uncompleted(Self {
            apfs_volume_label,
            existing_entry: ExistingFstabEntry::None,
        }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_fstab_entry")]
impl Action for CreateFstabEntry {
    fn action_tag() -> ActionTag {
        ActionTag("create_fstab_entry")
    }
    fn tracing_synopsis(&self) -> String {
        match self.existing_entry {
            ExistingFstabEntry::NixInstallerEntry | ExistingFstabEntry::Foreign => format!(
                "Update existing entry for the APFS volume `{}` to `/etc/fstab`",
                self.apfs_volume_label
            ),
            ExistingFstabEntry::None => format!(
                "Add a UUID based entry for the APFS volume `{}` to `/etc/fstab`",
                self.apfs_volume_label
            ),
        }
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_fstab_entry",
            apfs_volume_label = self.apfs_volume_label,
            existing_entry = ?self.existing_entry,
        );

        span
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            apfs_volume_label,
            existing_entry,
        } = self;
        let fstab_path = Path::new(FSTAB_PATH);
        let uuid = get_uuid_for_label(&apfs_volume_label).await?;

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

        let updated_buf = match existing_entry {
            ExistingFstabEntry::NixInstallerEntry => {
                // Update the entry
                let mut current_fstab_lines = fstab_buf
                    .lines()
                    .map(|v| v.to_owned())
                    .collect::<Vec<String>>();
                let mut updated_line = false;
                let mut saw_prelude = false;
                let prelude = fstab_prelude_comment(&apfs_volume_label);
                for line in current_fstab_lines.iter_mut() {
                    if line == &prelude {
                        saw_prelude = true;
                        continue;
                    }
                    if saw_prelude && line.split(&[' ', '\t']).nth(1) == Some("/nix") {
                        *line = fstab_entry(&uuid);
                        updated_line = true;
                        break;
                    }
                }
                if !(saw_prelude && updated_line) {
                    return Err(ActionError::Custom(Box::new(
                        CreateFstabEntryError::ExistingNixInstallerEntryDisappeared,
                    )));
                }
                current_fstab_lines.join("\n")
            },
            ExistingFstabEntry::Foreign => {
                // Overwrite the existing entry with our own
                let mut current_fstab_lines = fstab_buf
                    .lines()
                    .map(|v| v.to_owned())
                    .collect::<Vec<String>>();
                let mut updated_line = false;
                for line in current_fstab_lines.iter_mut() {
                    if line.split(&[' ', '\t']).nth(2) == Some("/nix") {
                        *line = fstab_lines(&uuid, apfs_volume_label);
                        updated_line = true;
                        break;
                    }
                }
                if !updated_line {
                    return Err(ActionError::Custom(Box::new(
                        CreateFstabEntryError::ExistingForeignEntryDisappeared,
                    )));
                }
                current_fstab_lines.join("\n")
            },
            ExistingFstabEntry::None => fstab_buf + "\n" + &fstab_lines(&uuid, apfs_volume_label),
        };

        fstab
            .seek(SeekFrom::Start(0))
            .await
            .map_err(|e| ActionError::Seek(fstab_path.to_owned(), e))?;
        fstab
            .set_len(0)
            .await
            .map_err(|e| ActionError::Truncate(fstab_path.to_owned(), e))?;
        fstab
            .write_all(updated_buf.as_bytes())
            .await
            .map_err(|e| ActionError::Write(fstab_path.to_owned(), e))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            apfs_volume_label,
            existing_entry: _,
        } = &self;
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
        let Self {
            apfs_volume_label,
            existing_entry: _,
        } = self;
        let fstab_path = Path::new(FSTAB_PATH);
        let uuid = get_uuid_for_label(&apfs_volume_label).await?;
        let fstab_entry = fstab_lines(&uuid, apfs_volume_label);

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
    .await?;

    let parsed: DiskUtilApfsInfoOutput = plist::from_bytes(&output.stdout)?;

    Ok(parsed.volume_uuid)
}

fn fstab_lines(uuid: &Uuid, apfs_volume_label: &str) -> String {
    let prelude_comment = fstab_prelude_comment(apfs_volume_label);
    let fstab_entry = fstab_entry(uuid);
    prelude_comment + "\n" + &fstab_entry
}

fn fstab_prelude_comment(apfs_volume_label: &str) -> String {
    format!("# nix-installer created volume labelled `{apfs_volume_label}`")
}

fn fstab_entry(uuid: &Uuid) -> String {
    format!("UUID={uuid} /nix apfs rw,noauto,nobrowse,suid,owners")
}

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum CreateFstabEntryError {
    #[error("The `/etc/fstab` entry (previously created by a `nix-installer` install) detected during planning disappeared between planning and executing. Cannot update `/etc/fstab` as planned")]
    ExistingNixInstallerEntryDisappeared,
    #[error("The `/etc/fstab` entry (previously created by the official install scripts) detected during planning disappeared between planning and executing. Cannot update `/etc/fstab` as planned")]
    ExistingForeignEntryDisappeared,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
struct DiskUtilApfsInfoOutput {
    #[serde(rename = "VolumeUUID")]
    volume_uuid: Uuid,
}
