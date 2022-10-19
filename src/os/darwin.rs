#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DiskUtilOutput {
    pub parent_whole_disk: String,
    pub global_permissions_enabled: bool,
}
