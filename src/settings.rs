/*! Configurable knobs and their related errors
*/
use std::collections::HashMap;

#[cfg(feature = "cli")]
use clap::ArgAction;
use url::Url;

use crate::channel_value::ChannelValue;

/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Linux x86_64
pub const NIX_X64_64_LINUX_URL: &str =
    "https://releases.nixos.org/nix/nix-2.12.0/nix-2.12.0-x86_64-linux.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Linux x86 (32 bit)
pub const NIX_I686_LINUX_URL: &str =
    "https://releases.nixos.org/nix/nix-2.12.0/nix-2.12.0-i686-linux.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Linux aarch64
pub const NIX_AARCH64_LINUX_URL: &str =
    "https://releases.nixos.org/nix/nix-2.12.0/nix-2.12.0-aarch64-linux.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Darwin x86_64
pub const NIX_X64_64_DARWIN_URL: &str =
    "https://releases.nixos.org/nix/nix-2.12.0/nix-2.12.0-x86_64-darwin.tar.xz";
/// Default [`nix_package_url`](CommonSettings::nix_package_url) for Darwin aarch64
pub const NIX_AARCH64_DARWIN_URL: &str =
    "https://releases.nixos.org/nix/nix-2.12.0/nix-2.12.0-aarch64-darwin.tar.xz";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Copy)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum InitSystem {
    #[cfg(not(target_os = "macos"))]
    None,
    #[cfg(target_os = "linux")]
    Systemd,
    #[cfg(target_os = "macos")]
    Launchd,
}

impl std::fmt::Display for InitSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(not(target_os = "macos"))]
            InitSystem::None => write!(f, "none"),
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => write!(f, "systemd"),
            #[cfg(target_os = "macos")]
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

    /// Modify the user profile to automatically load nix
    #[cfg_attr(
        feature = "cli",
        clap(
            action(ArgAction::SetFalse),
            default_value = "true",
            global = true,
            env = "NIX_INSTALLER_MODIFY_PROFILE",
            long = "no-modify-profile"
        )
    )]
    pub(crate) modify_profile: bool,

    /// Number of build users to create
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            default_value = "32",
            alias = "daemon-user-count",
            env = "NIX_INSTALLER_NIX_BUILD_USER_COUNT",
            global = true
        )
    )]
    pub(crate) nix_build_user_count: usize,

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
            default_value_t = 30_000,
            env = "NIX_INSTALLER_NIX_BUILD_GROUP_ID",
            global = true
        )
    )]
    pub(crate) nix_build_group_id: usize,

    /// The Nix build user prefix (user numbers will be postfixed)
    #[cfg_attr(
        feature = "cli",
        clap(long, env = "NIX_INSTALLER_NIX_BUILD_USER_PREFIX", global = true)
    )]
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
    #[cfg_attr(
        feature = "cli",
        clap(long, env = "NIX_INSTALLER_NIX_BUILD_USER_ID_BASE", global = true)
    )]
    // Service users on Mac should be between 200-400
    #[cfg_attr(all(target_os = "macos", feature = "cli"), clap(default_value_t = 300))]
    #[cfg_attr(
        all(target_os = "linux", feature = "cli"),
        clap(default_value_t = 30_000)
    )]
    pub(crate) nix_build_user_id_base: usize,

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
        all(target_os = "linux", target_arch = "x86", feature = "cli"),
        clap(
            default_value = NIX_I686_LINUX_URL,
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
    pub async fn default() -> Result<Self, InstallSettingsError> {
        let url;
        let nix_build_user_prefix;
        let nix_build_user_id_base;

        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            #[cfg(target_os = "linux")]
            (Architecture::X86_64, OperatingSystem::Linux) => {
                url = NIX_X64_64_LINUX_URL;
                nix_build_user_prefix = "nixbld";
                nix_build_user_id_base = 3000;
            },
            #[cfg(target_os = "linux")]
            (Architecture::X86_32(_), OperatingSystem::Linux) => {
                url = NIX_I686_LINUX_URL;
                nix_build_user_prefix = "nixbld";
                nix_build_user_id_base = 3000;
            },
            #[cfg(target_os = "linux")]
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                url = NIX_AARCH64_LINUX_URL;
                nix_build_user_prefix = "nixbld";
                nix_build_user_id_base = 3000;
            },
            #[cfg(target_os = "macos")]
            (Architecture::X86_64, OperatingSystem::MacOSX { .. })
            | (Architecture::X86_64, OperatingSystem::Darwin) => {
                url = NIX_X64_64_DARWIN_URL;
                nix_build_user_prefix = "_nixbld";
                nix_build_user_id_base = 300;
            },
            #[cfg(target_os = "macos")]
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
            nix_build_user_count: 32,
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
            nix_build_user_count,
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
            "nix_build_user_count".into(),
            serde_json::to_value(nix_build_user_count)?,
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
#[cfg(target_os = "linux")]
async fn linux_detect_init() -> (InitSystem, bool) {
    use std::process::Stdio;

    let mut detected = InitSystem::None;
    let mut started = false;
    if std::path::Path::new("/run/systemd/system").exists() {
        detected = InitSystem::Systemd;
        started = if tokio::process::Command::new("systemctl")
            .arg("status")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .ok()
            .map(|exit| exit.success())
            .unwrap_or(false)
        {
            true
        } else {
            false
        }
    }

    // TODO: Other inits
    (detected, started)
}

// Builder Pattern
impl CommonSettings {
    /// Number of build users to create
    pub fn nix_build_user_count(&mut self, count: usize) -> &mut Self {
        self.nix_build_user_count = count;
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

#[serde_with::serde_as]
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[cfg_attr(feature = "cli", derive(clap::Parser))]
pub struct InitSettings {
    /// Which init system to configure (if `--init none` Nix will be root-only)
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

    /// Start the daemon (if not `--init none`)
    #[cfg_attr(
        feature = "cli",
        clap(
            value_parser,
            long,
            action(ArgAction::SetFalse),
            env = "NIX_INSTALLER_START_DAEMON",
            default_value_t = true,
            long = "no-start-daemon"
        )
    )]
    pub(crate) start_daemon: bool,
}

impl InitSettings {
    /// The default settings for the given Architecture & Operating System
    pub async fn default() -> Result<Self, InstallSettingsError> {
        let init;
        let start_daemon;

        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            #[cfg(target_os = "linux")]
            (Architecture::X86_64, OperatingSystem::Linux) => {
                (init, start_daemon) = linux_detect_init().await;
            },
            #[cfg(target_os = "linux")]
            (Architecture::X86_32(_), OperatingSystem::Linux) => {
                (init, start_daemon) = linux_detect_init().await;
            },
            #[cfg(target_os = "linux")]
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                (init, start_daemon) = linux_detect_init().await;
            },
            #[cfg(target_os = "macos")]
            (Architecture::X86_64, OperatingSystem::MacOSX { .. })
            | (Architecture::X86_64, OperatingSystem::Darwin) => {
                (init, start_daemon) = (InitSystem::Launchd, true);
            },
            #[cfg(target_os = "macos")]
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. })
            | (Architecture::Aarch64(_), OperatingSystem::Darwin) => {
                (init, start_daemon) = (InitSystem::Launchd, true);
            },
            _ => {
                return Err(InstallSettingsError::UnsupportedArchitecture(
                    target_lexicon::HOST,
                ))
            },
        };

        Ok(Self { init, start_daemon })
    }

    /// A listing of the settings, suitable for [`Planner::settings`](crate::planner::Planner::settings)
    pub fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self { init, start_daemon } = self;
        let mut map = HashMap::default();

        map.insert("init".into(), serde_json::to_value(init)?);
        map.insert("start_daemon".into(), serde_json::to_value(start_daemon)?);
        Ok(map)
    }

    /// Which init system to configure
    pub fn init(&mut self, init: InitSystem) -> &mut Self {
        self.init = init;
        self
    }

    /// Start the daemon (if one is configured)
    pub fn start_daemon(&mut self, toggle: bool) -> &mut Self {
        self.start_daemon = toggle;
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
