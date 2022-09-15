use url::Url;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct InstallSettings {
    pub(crate) explain: bool,
    pub(crate) daemon_user_count: usize,
    pub(crate) channels: Vec<(String, Url)>,
    pub(crate) modify_profile: bool,
    pub(crate) nix_build_group_name: String,
    pub(crate) nix_build_group_id: usize,
    pub(crate) nix_build_user_prefix: String,
    pub(crate) nix_build_user_id_base: usize,
}

impl Default for InstallSettings {
    fn default() -> Self {
        Self {
            explain: Default::default(),
            daemon_user_count: Default::default(),
            channels: Default::default(),
            modify_profile: Default::default(),
            nix_build_group_name: String::from("nixbld"),
            nix_build_group_id: 3000,
            nix_build_user_prefix: String::from("nixbld"),
            nix_build_user_id_base: 3001,
        }
    }
}

// Builder Pattern
impl InstallSettings {
    pub fn explain(&mut self, explain: bool) -> &mut Self {
        self.explain = explain;
        self
    }
    pub fn daemon_user_count(&mut self, count: usize) -> &mut Self {
        self.daemon_user_count = count;
        self
    }

    pub fn channels(&mut self, channels: impl IntoIterator<Item = (String, Url)>) -> &mut Self {
        self.channels = channels.into_iter().collect();
        self
    }

    pub fn modify_profile(&mut self, toggle: bool) -> &mut Self {
        self.modify_profile = toggle;
        self
    }

    pub fn nix_build_group_name(&mut self, val: String) -> &mut Self {
        self.nix_build_group_name = val;
        self
    }

    pub fn nix_build_group_id(&mut self, count: usize) -> &mut Self {
        self.nix_build_group_id = count;
        self
    }

    pub fn nix_build_user_prefix(&mut self, val: String) -> &mut Self {
        self.nix_build_user_prefix = val;
        self
    }

    pub fn nix_build_user_id_base(&mut self, count: usize) -> &mut Self {
        self.nix_build_user_id_base = count;
        self
    }
}
