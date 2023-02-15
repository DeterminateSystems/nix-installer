#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DiskUtilOutput {
    pub parent_whole_disk: String,
    pub global_permissions_enabled: bool,
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
    pub name: String,
    pub encryption: bool,
}
