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
    pub encryption: bool,
}
