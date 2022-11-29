/*! Configurable knobs and their related errors
*/
use std::collections::HashMap;

#[cfg(feature = "cli")]
use clap::ArgAction;
use url::Url;

use crate::channel_value::ChannelValue;

/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Linux x86_64
pub const NIX_X64_64_LINUX_URL: &str =
    "https://releases.nixos.org/nix/nix-2.11.1/nix-2.11.1-x86_64-linux.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Linux aarch64
pub const NIX_AARCH64_LINUX_URL: &str =
    "https://releases.nixos.org/nix/nix-2.11.1/nix-2.11.1-aarch64-linux.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Darwin x86_64
pub const NIX_X64_64_DARWIN_URL: &str =
    "https://releases.nixos.org/nix/nix-2.11.1/nix-2.11.1-x86_64-darwin.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Darwin aarch64
pub const NIX_AARCH64_DARWIN_URL: &str =
    "https://releases.nixos.org/nix/nix-2.11.1/nix-2.11.1-aarch64-darwin.tar.xz";

/** Common settings used by all [`BuiltinPlanner`](crate::planner::BuiltinPlanner)s

Settings which only apply to certain [`Planner`](crate::planner::Planner)s should be located in the planner.

*/
#[serde_with::serde_as]
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct CommonSettings {
    /// Channel(s) to add
    #[cfg_attr(
        feature = "cli",clap(
        long,
        value_parser,
        name = "channel",
        action = clap::ArgAction::Append,
        env = "HARMONIC_CHANNEL",
        default_value = "nixpkgs=https://nixos.org/channels/nixpkgs-unstable",
    ))]
    pub(crate) channels: Vec<ChannelValue>,

    /// Modify the user profile to automatically load nix
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            action(ArgAction::SetFalse),
            default_value = "true",
            global = true,
            env = "HARMONIC_NO_MODIFY_PROFILE",
            name = "no-modify-profile"
        )
    )]
    pub(crate) modify_profile: bool,

    /// Number of build users to create
    #[cfg_attr(
        feature = "cli",
        clap(long, default_value = "32", env = "HARMONIC_DAEMON_USER_COUNT")
    )]
    pub(crate) daemon_user_count: usize,

    /// The Nix build group name
    #[cfg_attr(
        feature = "cli",
        clap(long, default_value = "nixbld", env = "HARMONIC_NIX_BUILD_GROUP_NAME")
    )]
    pub(crate) nix_build_group_name: String,

    /// The Nix build group GID
    #[cfg_attr(
        feature = "cli",
        clap(long, default_value_t = 3000, env = "HARMONIC_NIX_BUILD_GROUP_ID")
    )]
    pub(crate) nix_build_group_id: usize,

    /// The Nix build user prefix (user numbers will be postfixed)
    #[cfg_attr(feature = "cli", clap(long, env = "HARMONIC_NIX_BUILD_USER_PREFIX"))]
    #[cfg_attr(
        all(target_os = "macos", feature = "cli"),
        clap(default_value = "_nixbld")
    )]
    #[cfg_attr(
        all(target_os = "linux", feature = "cli"),
        clap(default_value = "nixbld")
    )]
    pub(crate) nix_build_user_prefix: String,

    /// The Nix build user base UID (ascending)
    #[cfg_attr(feature = "cli", clap(long, env = "HARMONIC_NIX_BUILD_USER_ID_BASE"))]
    #[cfg_attr(all(target_os = "macos", feature = "cli"), clap(default_value_t = 300))]
    #[cfg_attr(
        all(target_os = "linux", feature = "cli"),
        clap(default_value_t = 3000)
    )]
    pub(crate) nix_build_user_id_base: usize,

    /// The Nix package URL
    #[cfg_attr(feature = "cli", clap(long, env = "HARMONIC_NIX_PACKAGE_URL"))]
    #[cfg_attr(
        all(target_os = "macos", target_arch = "x86_64", feature = "cli"),
        clap(
            default_value = NIX_X64_64_DARWIN_URL,
        )
    )]
    #[cfg_attr(
        all(target_os = "macos", target_arch = "aarch64", feature = "cli"),
        clap(
            default_value = NIX_AARCH64_DARWIN_URL,
        )
    )]
    #[cfg_attr(
        all(target_os = "linux", target_arch = "x86_64", feature = "cli"),
        clap(
            default_value = NIX_X64_64_LINUX_URL,
        )
    )]
    #[cfg_attr(
        all(target_os = "linux", target_arch = "aarch64", feature = "cli"),
        clap(
            default_value = NIX_AARCH64_LINUX_URL,
        )
    )]
    pub(crate) nix_package_url: Url,

    /// Extra configuration lines for `/etc/nix.conf`
    #[cfg_attr(feature = "cli", clap(long, env = "HARMONIC_EXTRA_CONF"))]
    pub(crate) extra_conf: Option<String>,

    /// If Harmonic should forcibly recreate files it finds existing
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            action(ArgAction::SetTrue),
            default_value = "false",
            global = true,
            env = "HARMONIC_FORCE"
        )
    )]
    pub(crate) force: bool,
}

impl CommonSettings {
    /// The default settings for the given Architecture & Operating System
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
            daemon_user_count: 32,
            channels: vec![ChannelValue(
                "nixpkgs".into(),
                reqwest::Url::parse("https://nixos.org/channels/nixpkgs-unstable")
                    .expect("Embedded default URL was not a URL, please report this"),
            )],
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

    /// A listing of the settings, suitable for [`Planner::settings`](crate::planner::Planner::settings)
    pub fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
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
    /// Number of build users to create
    pub fn daemon_user_count(&mut self, count: usize) -> &mut Self {
        self.daemon_user_count = count;
        self
    }

    /// Channel(s) to add
    pub fn channels(&mut self, channels: impl IntoIterator<Item = (String, Url)>) -> &mut Self {
        self.channels = channels.into_iter().map(Into::into).collect();
        self
    }

    /// Modify the user profile to automatically load nix
    pub fn modify_profile(&mut self, toggle: bool) -> &mut Self {
        self.modify_profile = toggle;
        self
    }

    /// The Nix build group name
    pub fn nix_build_group_name(&mut self, val: String) -> &mut Self {
        self.nix_build_group_name = val;
        self
    }

    /// The Nix build group GID
    pub fn nix_build_group_id(&mut self, count: usize) -> &mut Self {
        self.nix_build_group_id = count;
        self
    }

    /// The Nix build user prefix (user numbers will be postfixed)
    pub fn nix_build_user_prefix(&mut self, val: String) -> &mut Self {
        self.nix_build_user_prefix = val;
        self
    }

    /// The Nix build user base UID (ascending)
    pub fn nix_build_user_id_base(&mut self, count: usize) -> &mut Self {
        self.nix_build_user_id_base = count;
        self
    }

    /// The Nix package URL
    pub fn nix_package_url(&mut self, url: Url) -> &mut Self {
        self.nix_package_url = url;
        self
    }

    /// Extra configuration lines for `/etc/nix.conf`
    pub fn extra_conf(&mut self, extra_conf: Option<String>) -> &mut Self {
        self.extra_conf = extra_conf;
        self
    }

    /// If Harmonic should forcibly recreate files it finds existing
    pub fn force(&mut self, force: bool) -> &mut Self {
        self.force = force;
        self
    }
}

/// An error originating from a [`Planner::settings`](crate::planner::Planner::settings)
#[derive(thiserror::Error, Debug)]
pub enum InstallSettingsError {
    /// Harmonic does not support the architecture right now
    #[error("Harmonic does not support the `{0}` architecture right now")]
    UnsupportedArchitecture(target_lexicon::Triple),
    /// Parsing URL
    #[error("Parsing URL")]
    Parse(
        #[source]
        #[from]
        url::ParseError,
    ),
    /// JSON serialization or deserialization error
    #[error("JSON serialization or deserialization error")]
    SerdeJson(
        #[source]
        #[from]
        serde_json::Error,
    ),
}
