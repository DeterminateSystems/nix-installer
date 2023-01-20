/*! Configurable knobs and their related errors
*/
use std::{collections::HashMap, path::Path};

#[cfg(feature = "cli")]
use clap::ArgAction;
use url::Url;

use crate::{channel_value::ChannelValue, planner::PlannerError};

/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Linux x86_64
pub const NIX_X64_64_LINUX_URL: &str =
    "https://releases.nixos.org/nix/nix-2.13.0/nix-2.13.0-x86_64-linux.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Linux aarch64
pub const NIX_AARCH64_LINUX_URL: &str =
    "https://releases.nixos.org/nix/nix-2.13.0/nix-2.13.0-aarch64-linux.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Darwin x86_64
pub const NIX_X64_64_DARWIN_URL: &str =
    "https://releases.nixos.org/nix/nix-2.13.0/nix-2.13.0-x86_64-darwin.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Darwin aarch64
pub const NIX_AARCH64_DARWIN_URL: &str =
    "https://releases.nixos.org/nix/nix-2.13.0/nix-2.13.0-aarch64-darwin.tar.xz";

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize, Debug, clap::ValueEnum)]
pub enum InitSystem {
    None,
    Systemd,
    Launchd,
}

impl std::fmt::Display for InitSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitSystem::None => write!(f, "none"),
            InitSystem::Systemd => write!(f, "systemd"),
            InitSystem::Launchd => write!(f, "launchd"),
        }
    }
}

/** Common settings used by all [`BuiltinPlanner`](crate::planner::BuiltinPlanner)s

Settings which only apply to certain [`Planner`](crate::planner::Planner)s should be located in the planner.

*/
#[serde_with::serde_as]
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct CommonSettings {
    /// Channel(s) to add, for no default channel, pass `--channel`
    #[cfg_attr(
        feature = "cli",
        clap(
            value_parser,
            long = "channel",
            num_args = 0..,
            action = clap::ArgAction::Append,
            env = "NIX_INSTALLER_CHANNELS",
            default_value = "nixpkgs=https://nixos.org/channels/nixpkgs-unstable",
        )
    )]
    pub(crate) channels: Vec<ChannelValue>,

    #[cfg_attr(feature = "cli", clap(value_parser, long, env = "NIX_INSTALLER_INIT",))]
    #[cfg_attr(
        all(target_os = "macos", feature = "cli"),
        clap(default_value_t = InitSystem::Launchd)
    )]
    #[cfg_attr(
        all(target_os = "linux", feature = "cli"),
        clap(default_value_t = InitSystem::Systemd)
    )]
    pub(crate) init: InitSystem,

    /// Modify the user profile to automatically load nix
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            action(ArgAction::SetFalse),
            default_value = "true",
            global = true,
            env = "NIX_INSTALLER_NO_MODIFY_PROFILE",
            name = "no-modify-profile"
        )
    )]
    pub(crate) modify_profile: bool,

    /// The Nix build group name
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            default_value = "nixbld",
            env = "NIX_INSTALLER_NIX_BUILD_GROUP_NAME",
            global = true
        )
    )]
    pub(crate) nix_build_group_name: String,

    /// The Nix build group GID
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            default_value_t = 3000,
            env = "NIX_INSTALLER_NIX_BUILD_GROUP_ID",
            global = true
        )
    )]
    pub(crate) nix_build_group_id: usize,

    /// The Nix package URL
    #[cfg_attr(
        feature = "cli",
        clap(long, env = "NIX_INSTALLER_NIX_PACKAGE_URL", global = true)
    )]
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
    #[cfg_attr(feature = "cli", clap(long, action = ArgAction::Set, num_args = 0.., value_delimiter = ',', env = "NIX_INSTALLER_EXTRA_CONF", global = true))]
    pub extra_conf: Vec<String>,

    /// If `nix-installer` should forcibly recreate files it finds existing
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            action(ArgAction::SetTrue),
            default_value = "false",
            global = true,
            env = "NIX_INSTALLER_FORCE"
        )
    )]
    pub(crate) force: bool,
}

impl CommonSettings {
    /// The default settings for the given Architecture & Operating System
    pub fn default() -> Result<Self, InstallSettingsError> {
        let url;
        let init;

        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            (Architecture::X86_64, OperatingSystem::Linux) => {
                url = NIX_X64_64_LINUX_URL;
                init = linux_detect_init()?;
            },
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                url = NIX_AARCH64_LINUX_URL;
                init = linux_detect_init()?;
            },
            (Architecture::X86_64, OperatingSystem::MacOSX { .. })
            | (Architecture::X86_64, OperatingSystem::Darwin) => {
                url = NIX_X64_64_DARWIN_URL;
                init = InitSystem::Launchd;
            },
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. })
            | (Architecture::Aarch64(_), OperatingSystem::Darwin) => {
                url = NIX_AARCH64_DARWIN_URL;
                init = InitSystem::Launchd;
            },
            _ => {
                return Err(InstallSettingsError::UnsupportedArchitecture(
                    target_lexicon::HOST,
                ))
            },
        };

        Ok(Self {
            channels: vec![ChannelValue(
                "nixpkgs".into(),
                reqwest::Url::parse("https://nixos.org/channels/nixpkgs-unstable")
                    .expect("Embedded default URL was not a URL, please report this"),
            )],
            init,
            modify_profile: true,
            nix_build_group_name: String::from("nixbld"),
            nix_build_group_id: 3000,
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
            nix_build_group_name,
            nix_build_group_id,
            nix_package_url,
            init,
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
        map.insert("init".into(), serde_json::to_value(init)?);
        map.insert(
            "modify_profile".into(),
            serde_json::to_value(modify_profile)?,
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
            "nix_package_url".into(),
            serde_json::to_value(nix_package_url)?,
        );
        map.insert("extra_conf".into(), serde_json::to_value(extra_conf)?);
        map.insert("force".into(), serde_json::to_value(force)?);

        Ok(map)
    }
}

fn linux_detect_init() -> Result<InitSystem, InstallSettingsError> {
    let mut detected = None;
    if !Path::new("/run/systemd/system").exists() {
        detected = Some(InitSystem::Systemd)
    }
    // TODO: Other inits

    if let Some(detected) = detected {
        return Ok(detected);
    } else {
        return Err(InstallSettingsError::InitNotSupported);
    }
}

// Builder Pattern
impl CommonSettings {
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

    /// The Nix package URL
    pub fn nix_package_url(&mut self, url: Url) -> &mut Self {
        self.nix_package_url = url;
        self
    }
    /// Extra configuration lines for `/etc/nix.conf`
    pub fn extra_conf(&mut self, extra_conf: Vec<String>) -> &mut Self {
        self.extra_conf = extra_conf;
        self
    }

    /// If `nix-installer` should forcibly recreate files it finds existing
    pub fn force(&mut self, force: bool) -> &mut Self {
        self.force = force;
        self
    }
}

/// An error originating from a [`Planner::settings`](crate::planner::Planner::settings)
#[derive(thiserror::Error, Debug)]
pub enum InstallSettingsError {
    /// `nix-installer` does not support the architecture right now
    #[error("`nix-installer` does not support the `{0}` architecture right now")]
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
    #[error("No supported init system found")]
    InitNotSupported,
}
