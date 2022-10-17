use crate::planner;
use target_lexicon::Triple;
use url::Url;

#[serde_with::serde_as]
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct InstallSettings {
    pub(crate) daemon_user_count: usize,
    pub(crate) channels: Vec<(String, Url)>,
    pub(crate) modify_profile: bool,
    pub(crate) nix_build_group_name: String,
    pub(crate) nix_build_group_id: usize,
    pub(crate) nix_build_user_prefix: String,
    pub(crate) nix_build_user_id_base: usize,
    pub(crate) nix_package_url: Url,
    pub(crate) extra_conf: Option<String>,
    pub(crate) force: bool,
    #[serde_as(as = "serde_with::DisplayFromStr")]
    pub(crate) triple: Triple,
}

impl InstallSettings {
    pub fn default() -> Result<Self, InstallSettingsError> {
        let url;

        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            (Architecture::X86_64, OperatingSystem::Linux) => {
                url = "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-x86_64-linux.tar.xz";
            },
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                url = "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-aarch64-linux.tar.xz";
            },
            (Architecture::X86_64, OperatingSystem::MacOSX { .. }) => {
                url = "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-x86_64-darwin.tar.xz";
            },
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. }) => {
                url = "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-aarch64-darwin.tar.xz";
            },
            _ => {
                return Err(InstallSettingsError::UnsupportedArchitecture(
                    target_lexicon::HOST,
                ))
            },
        };

        Ok(Self {
            triple: target_lexicon::HOST,
            daemon_user_count: Default::default(),
            channels: Default::default(),
            modify_profile: Default::default(),
            nix_build_group_name: String::from("nixbld"),
            nix_build_group_id: 3000,
            nix_build_user_prefix: String::from("nixbld"),
            nix_build_user_id_base: 3001,
            nix_package_url: url
                .parse()
                .expect("Could not parse default Nix archive url, please report this issue"),
            extra_conf: Default::default(),
            force: false,
        })
    }
}

// Builder Pattern
impl InstallSettings {
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
    pub fn nix_package_url(&mut self, url: Url) -> &mut Self {
        self.nix_package_url = url;
        self
    }
    pub fn extra_conf(&mut self, extra_conf: Option<String>) -> &mut Self {
        self.extra_conf = extra_conf;
        self
    }
    pub fn force(&mut self, force: bool) -> &mut Self {
        self.force = force;
        self
    }
}

#[derive(thiserror::Error, Debug)]
pub enum InstallSettingsError {
    #[error("Harmonic does not support the `{0}` architecture right now")]
    UnsupportedArchitecture(target_lexicon::Triple),
    #[error("Planner error")]
    Planner(
        #[source]
        #[from]
        planner::PlannerError,
    ),
}
