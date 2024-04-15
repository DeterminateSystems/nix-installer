use std::path::PathBuf;

#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DiskUtilInfoOutput {
    pub parent_whole_disk: String,
    pub global_permissions_enabled: bool,
    pub mount_point: Option<PathBuf>,
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
