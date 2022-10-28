use std::collections::HashMap;

use clap::ArgAction;
use url::Url;

use crate::cli::arg::ChannelValue;

pub const NIX_X64_64_LINUX_URL: &str =
    "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-x86_64-linux.tar.xz";
pub const NIX_AARCH64_LINUX_URL: &str =
    "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-aarch64-linux.tar.xz";
pub const NIX_X64_64_DARWIN_URL: &str =
    "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-x86_64-darwin.tar.xz";
pub const NIX_AARCH64_DARWIN_URL: &str =
    "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-aarch64-darwin.tar.xz";

#[serde_with::serde_as]
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, clap::Parser)]
pub struct CommonSettings {
    /// Channel(s) to add by default, pass multiple times for multiple channels
    #[clap(
        long,
        value_parser,
        name = "channel",
        action = clap::ArgAction::Append,
        env = "HARMONIC_CHANNEL",
        default_value = "nixpkgs=https://nixos.org/channels/nixpkgs-unstable",
    )]
    pub(crate) channels: Vec<crate::cli::arg::ChannelValue>,

    /// Modify the user profile to automatically load nix
    #[clap(
        long,
        action(ArgAction::SetFalse),
        default_value = "true",
        global = true,
        env = "HARMONIC_NO_MODIFY_PROFILE",
        name = "no-modify-profile"
    )]
    pub(crate) modify_profile: bool,

    /// Number of build users to create
    #[clap(long, default_value = "32", env = "HARMONIC_DAEMON_USER_COUNT")]
    pub(crate) daemon_user_count: usize,

    #[clap(long, default_value = "nixbld", env = "HARMONIC_NIX_BUILD_GROUP_NAME")]
    pub(crate) nix_build_group_name: String,

    #[clap(long, default_value_t = 3000, env = "HARMONIC_NIX_BUILD_GROUP_ID")]
    pub(crate) nix_build_group_id: usize,

    #[clap(long, env = "HARMONIC_NIX_BUILD_USER_PREFIX")]
    #[cfg_attr(target_os = "macos", clap(default_value = "_nixbld"))]
    #[cfg_attr(target_os = "linux", clap(default_value = "nixbld"))]
    pub(crate) nix_build_user_prefix: String,

    #[clap(long, env = "HARMONIC_NIX_BUILD_USER_ID_BASE")]
    #[cfg_attr(target_os = "macos", clap(default_value_t = 300))]
    #[cfg_attr(target_os = "linux", clap(default_value_t = 3000))]
    pub(crate) nix_build_user_id_base: usize,

    #[clap(long, env = "HARMONIC_NIX_PACKAGE_URL")]
    #[cfg_attr(
        all(target_os = "macos", target_arch = "x86_64"),
        clap(
            default_value = NIX_X64_64_DARWIN_URL,
        )
    )]
    #[cfg_attr(
        all(target_os = "macos", target_arch = "aarch64"),
        clap(
            default_value = NIX_AARCH64_DARWIN_URL,
        )
    )]
    #[cfg_attr(
        all(target_os = "linux", target_arch = "x86_64"),
        clap(
            default_value = NIX_X64_64_LINUX_URL,
        )
    )]
    #[cfg_attr(
        all(target_os = "linux", target_arch = "aarch64"),
        clap(
            default_value = NIX_AARCH64_LINUX_URL,
        )
    )]
    pub(crate) nix_package_url: Url,

    #[clap(long, env = "HARMONIC_EXTRA_CONF")]
    pub(crate) extra_conf: Option<String>,

    #[clap(
        long,
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true,
        env = "HARMONIC_FORCE"
    )]
    pub(crate) force: bool,
}

impl CommonSettings {
    pub fn default() -> Result<Self, InstallSettingsError> {
        let url;
        let nix_build_user_prefix;
        let nix_build_user_id_base;

        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            (Architecture::X86_64, OperatingSystem::Linux) => {
                url = NIX_X64_64_LINUX_URL;
                nix_build_user_prefix = "nixbld";
                nix_build_user_id_base = 3000;
            },
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                url = NIX_AARCH64_LINUX_URL;
                nix_build_user_prefix = "nixbld";
                nix_build_user_id_base = 3000;
            },
            (Architecture::X86_64, OperatingSystem::MacOSX { .. })
            | (Architecture::X86_64, OperatingSystem::Darwin) => {
                url = NIX_X64_64_DARWIN_URL;
                nix_build_user_prefix = "_nixbld";
                nix_build_user_id_base = 300;
            },
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. })
            | (Architecture::Aarch64(_), OperatingSystem::Darwin) => {
                url = NIX_AARCH64_DARWIN_URL;
                nix_build_user_prefix = "_nixbld";
                nix_build_user_id_base = 300;
            },
            _ => {
                return Err(InstallSettingsError::UnsupportedArchitecture(
                    target_lexicon::HOST,
                ))
            },
        };

        Ok(Self {
            daemon_user_count: Default::default(),
            channels: Vec::default(),
            modify_profile: true,
            nix_build_group_name: String::from("nixbld"),
            nix_build_group_id: 3000,
            nix_build_user_prefix: nix_build_user_prefix.to_string(),
            nix_build_user_id_base,
            nix_package_url: url.parse()?,
            extra_conf: Default::default(),
            force: false,
        })
    }

    pub fn describe(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, Box<dyn std::error::Error + Sync + Send>> {
        let Self {
            channels,
            modify_profile,
            daemon_user_count,
            nix_build_group_name,
            nix_build_group_id,
            nix_build_user_prefix,
            nix_build_user_id_base,
            nix_package_url,
            extra_conf,
            force,
        } = self;
        let mut map = HashMap::default();

        map.insert(
            "channels".into(),
            serde_json::to_value(
                channels
                    .iter()
                    .map(|ChannelValue(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>(),
            )?,
        );
        map.insert(
            "modify_profile".into(),
            serde_json::to_value(modify_profile)?,
        );
        map.insert(
            "daemon_user_count".into(),
            serde_json::to_value(daemon_user_count)?,
        );
        map.insert(
            "nix_build_group_name".into(),
            serde_json::to_value(nix_build_group_name)?,
        );
        map.insert(
            "nix_build_group_id".into(),
            serde_json::to_value(nix_build_group_id)?,
        );
        map.insert(
            "nix_build_user_prefix".into(),
            serde_json::to_value(nix_build_user_prefix)?,
        );
        map.insert(
            "nix_build_user_id_base".into(),
            serde_json::to_value(nix_build_user_id_base)?,
        );
        map.insert(
            "nix_package_url".into(),
            serde_json::to_value(nix_package_url)?,
        );
        map.insert("extra_conf".into(), serde_json::to_value(extra_conf)?);
        map.insert("force".into(), serde_json::to_value(force)?);

        Ok(map)
    }
}

// Builder Pattern
impl CommonSettings {
    pub fn daemon_user_count(&mut self, count: usize) -> &mut Self {
        self.daemon_user_count = count;
        self
    }

    pub fn channels(&mut self, channels: impl IntoIterator<Item = (String, Url)>) -> &mut Self {
        self.channels = channels.into_iter().map(Into::into).collect();
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
    #[error("Parsing URL")]
    Parse(
        #[source]
        #[from]
        url::ParseError,
    ),
}
