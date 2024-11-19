use std::path::PathBuf;

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DiskUtilInfoOutput {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub parent_whole_disk: String,
    pub global_permissions_enabled: bool,
    pub mount_point: Option<PathBuf>,
}

impl DiskUtilInfoOutput {
    pub async fn for_volume_name(
        volume_name: &str,
    ) -> Result<Self, crate::action::ActionErrorKind> {
        Self::for_volume_path(std::path::Path::new(volume_name)).await
    }

    pub async fn for_volume_path(
        volume_path: &std::path::Path,
    ) -> Result<Self, crate::action::ActionErrorKind> {
        let buf = crate::execute_command(
            tokio::process::Command::new("/usr/sbin/diskutil")
                .process_group(0)
                .args(["info", "-plist"])
                .arg(volume_path)
                .stdin(std::process::Stdio::null()),
        )
        .await?
        .stdout;

        Ok(plist::from_reader(std::io::Cursor::new(buf))?)
    }

    pub fn is_mounted(&self) -> bool {
        match self.mount_point {
            None => false,
            Some(ref mp) => !mp.as_os_str().is_empty(),
        }
    }
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct DiskUtilApfsListOutput {
    pub containers: Vec<DiskUtilApfsContainer>,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct DiskUtilApfsContainer {
    pub volumes: Vec<DiskUtilApfsListVolume>,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct DiskUtilApfsListVolume {
    pub name: Option<String>,
    pub file_vault: bool,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct DiskUtilList {
    pub all_disks_and_partitions: Vec<DiskUtilListDisk>,
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct DiskUtilListDisk {
    #[serde(rename = "OSInternal")]
    pub os_internal: bool,
    pub device_identifier: String,
    #[serde(rename = "Size")]
    pub size_bytes: u64,
}
