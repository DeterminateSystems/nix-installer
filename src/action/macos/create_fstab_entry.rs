use std::path::Path;

use tracing::{span, Span};
use uuid::Uuid;

use super::get_disk_info_for_label;
use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

const FSTAB_PATH: &str = "/etc/fstab";

/** Create an `/etc/fstab` entry for the given volume

This action queries `diskutil info` on the volume to fetch it's UUID and
add the relevant information to `/etc/fstab`.
 */
// Initially, a `NAME` was used, however in https://github.com/DeterminateSystems/nix-installer/issues/212
// several users reported issues. Using a UUID resolved the issue for them.
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_fstab_entry")]
pub struct CreateFstabEntry {
    apfs_volume_label: String,
}

impl CreateFstabEntry {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(apfs_volume_label: String) -> Result<StatefulAction<Self>, ActionError> {
        Ok(StatefulAction::uncompleted(Self { apfs_volume_label }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_fstab_entry")]
impl Action for CreateFstabEntry {
    fn action_tag() -> ActionTag {
        ActionTag("create_fstab_entry")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Update `{FSTAB_PATH}` to mount the APFS volume `{}`",
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
        let fstab_path = Path::new(FSTAB_PATH);
        let uuid = match get_disk_info_for_label(&self.apfs_volume_label)
            .await
            .map_err(Self::error)?
        {
            Some(diskutil_info) => diskutil_info.volume_uuid,
            None => {
                return Err(Self::error(CreateFstabEntryError::CannotDetermineUuid(
                    self.apfs_volume_label.clone(),
                )))?
            },
        };

        let fstab_buf = tokio::fs::read_to_string(FSTAB_PATH)
            .await
            .or_else(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Ok(String::new()),
                _ => Err(e),
            })
            .map_err(|e| Self::error(ActionErrorKind::Read(fstab_path.to_owned(), e)))?;

        let mut current_fstab_lines = fstab_buf
            .lines()
            .filter(|line| {
                // Remove nix-installer entries with a "prelude" comment
                if line.starts_with("# nix-installer created volume labelled") {
                    return false;
                }
                // Remove any existing /nix mount point entries
                if line.split(&[' ', '\t']).nth(1) == Some("/nix") {
                    return false;
                }
                true
            })
            .map(|line| line.to_owned())
            .collect::<Vec<String>>();

        // Always append exactly one new /nix entry
        current_fstab_lines.push(fstab_entry(&uuid));

        if current_fstab_lines.last().map(|s| s.as_ref()) != Some("") {
            // Don't leave the file without a trailing newline
            current_fstab_lines.push("".into());
        }

        let updated_buf = current_fstab_lines.join("\n");

        crate::util::write_atomic(fstab_path, &updated_buf)
            .await
            .map_err(Self::error)?;
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
        let fstab_path = Path::new(FSTAB_PATH);

        let fstab_buf = tokio::fs::read_to_string(FSTAB_PATH)
            .await
            .or_else(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Ok(String::new()),
                _ => Err(e),
            })
            .map_err(|e| Self::error(ActionErrorKind::Read(fstab_path.to_owned(), e)))?;

        let mut current_fstab_lines = fstab_buf
            .lines()
            .filter_map(|line| {
                // Delete nix-installer entries with a "prelude" comment
                if line.starts_with("# nix-installer created volume labelled") {
                    None
                } else {
                    Some(line)
                }
            })
            .filter_map(|line| {
                if line.split(&[' ', '\t']).nth(1) == Some("/nix") {
                    // Delete the mount line for /nix
                    None
                } else {
                    Some(line)
                }
            })
            .collect::<Vec<&str>>();

        if current_fstab_lines.last() != Some(&"") {
            // Don't leave the file without a trailing newline
            current_fstab_lines.push("");
        }

        crate::util::write_atomic(fstab_path, &current_fstab_lines.join("\n"))
            .await
            .map_err(Self::error)?;

        Ok(())
    }
}

fn fstab_entry(uuid: &Uuid) -> String {
    format!("UUID={uuid} /nix apfs rw,noatime,noauto,nobrowse,nosuid,owners # Added by the Determinate Nix Installer")
}

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum CreateFstabEntryError {
    #[error("Unable to determine how to add APFS volume `{0}` the `/etc/fstab` line, likely the volume is not yet created or there is some synchronization issue, please report this")]
    CannotDetermineUuid(String),
}

impl From<CreateFstabEntryError> for ActionErrorKind {
    fn from(val: CreateFstabEntryError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}
