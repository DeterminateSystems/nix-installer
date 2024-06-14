/*! Configurable knobs and their related errors
*/
use std::{collections::HashMap, fmt::Display, path::PathBuf, str::FromStr};

#[cfg(feature = "cli")]
use clap::{
    error::{ContextKind, ContextValue},
    ArgAction,
};
use color_eyre::owo_colors::OwoColorize as _;
use eyre::Context as _;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use url::Url;

pub const SCRATCH_DIR: &str = "/nix/temp-install-dir";

pub const NIX_TARBALL_PATH: &str = env!("NIX_INSTALLER_TARBALL_PATH");
/// The NIX_INSTALLER_TARBALL_PATH environment variable should point to a target-appropriate
/// Nix installation tarball, like nix-2.21.2-aarch64-darwin.tar.xz. The contents are embedded
/// in the resulting binary.
pub const NIX_TARBALL: &[u8] = include_bytes!(env!("NIX_INSTALLER_TARBALL_PATH"));

#[cfg(all(feature = "determinate-nix", target_os = "linux"))]
/// The DETERMINATE_NIX_BINARY_PATH environment variable should point to a target-appropriate
/// static build of the Determinate Nix shim binary. The contents are embedded in the resulting
/// binary if the determinate-nix feature is turned on.
pub const DETERMINATE_NIX_BINARY: Option<&[u8]> =
    Some(include_bytes!(env!("DETERMINATE_NIX_BINARY_PATH")));

#[cfg(any(not(feature = "determinate-nix"), not(target_os = "linux")))]
/// The DETERMINATE_NIX_BINARY_PATH environment variable should point to a target-appropriate
/// static build of the Determinate Nix shim binary. The contents are embedded in the resulting
/// binary if the determinate-nix feature is turned on.
pub const DETERMINATE_NIX_BINARY: Option<&[u8]> = None;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
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
    /// Enable Determinate Nix Enterprise Edition. See: <https://determinate.systems/enterprise>
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            env = "NIX_INSTALLER_ENTERPRISE_EDITION",
            default_value = "false"
        )
    )]
    pub enterprise_edition: bool,

    /// Modify the user profile to automatically load Nix
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
    pub modify_profile: bool,

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
    pub nix_build_group_name: String,

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
    pub nix_build_group_id: u32,

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
    pub nix_build_user_prefix: String,

    /// The number of build users to create
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            alias = "daemon-user-count",
            env = "NIX_INSTALLER_NIX_BUILD_USER_COUNT",
            global = true
        )
    )]
    #[cfg_attr(all(target_os = "macos", feature = "cli"), clap(default_value = "32"))]
    #[cfg_attr(all(target_os = "linux", feature = "cli"), clap(default_value = "32"))]
    pub nix_build_user_count: u32,

    /// The Nix build user base UID (ascending)
    #[cfg_attr(
        feature = "cli",
        clap(long, env = "NIX_INSTALLER_NIX_BUILD_USER_ID_BASE", global = true)
    )]
    #[cfg_attr(
        all(target_os = "macos", feature = "cli"),
        doc = "Service users on Mac should be between 200-400"
    )]
    #[cfg_attr(
        all(feature = "cli"),
        clap(default_value_t = default_nix_build_user_id_base())
    )]
    pub nix_build_user_id_base: u32,

    /// The Nix package URL
    #[cfg_attr(
        feature = "cli",
        clap(long, env = "NIX_INSTALLER_NIX_PACKAGE_URL", global = true, value_parser = clap::value_parser!(UrlOrPath), default_value = None)
    )]
    pub nix_package_url: Option<UrlOrPath>,

    /// The proxy to use (if any); valid proxy bases are `https://$URL`, `http://$URL` and `socks5://$URL`
    #[cfg_attr(feature = "cli", clap(long, env = "NIX_INSTALLER_PROXY"))]
    pub proxy: Option<Url>,

    /// An SSL cert to use (if any); used for fetching Nix and sets `ssl-cert-file` in `/etc/nix/nix.conf`
    #[cfg_attr(feature = "cli", clap(long, env = "NIX_INSTALLER_SSL_CERT_FILE"))]
    pub ssl_cert_file: Option<PathBuf>,

    /// Extra configuration lines for `/etc/nix.conf`
    #[cfg_attr(feature = "cli", clap(long, action = ArgAction::Append, num_args = 0.., env = "NIX_INSTALLER_EXTRA_CONF", global = true))]
    pub extra_conf: Vec<UrlOrPathOrString>,

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
    pub force: bool,

    #[cfg(feature = "diagnostics")]
    /// Relate the install diagnostic to a specific value
    #[cfg_attr(
        feature = "cli",
        clap(
            long,
            default_value = None,
            env = "NIX_INSTALLER_DIAGNOSTIC_ATTRIBUTION",
            global = true
        )
    )]
    pub diagnostic_attribution: Option<String>,

    #[cfg(feature = "diagnostics")]
    /// The URL or file path for an installation diagnostic to be sent
    ///
    /// Sample of the data sent:
    ///
    /// {
    ///     "attribution": null,
    ///     "version": "0.4.0",
    ///     "planner": "linux",
    ///     "configured_settings": [ "modify_profile" ],
    ///     "os_name": "Ubuntu",
    ///     "os_version": "22.04.1 LTS (Jammy Jellyfish)",
    ///     "triple": "x86_64-unknown-linux-gnu",
    ///     "is_ci": false,
    ///     "action": "Install",
    ///     "status": "Success"
    /// }
    ///
    /// To disable diagnostic reporting, unset the default with `--diagnostic-endpoint ""`, or `NIX_INSTALLER_DIAGNOSTIC_ENDPOINT=""`
    #[clap(
        long,
        env = "NIX_INSTALLER_DIAGNOSTIC_ENDPOINT",
        global = true,
        value_parser = crate::diagnostics::diagnostic_endpoint_validator,
        num_args = 0..=1, // Required to allow `--diagnostic-endpoint` or `NIX_INSTALLER_DIAGNOSTIC_ENDPOINT=""`
        default_value = "https://install.determinate.systems/nix/diagnostic"
    )]
    pub diagnostic_endpoint: Option<String>,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct SystemVersionPlist {
    product_version: String,
}

const MACOS_SYSTEM_VERSION_PLIST_PATH: &str = "/System/Library/CoreServices/SystemVersion.plist";
const MACOS_SYSTEM_VERSION_PLIST_SYMLINK_PATH: &str =
    "/System/Library/CoreServices/.SystemVersionPlatform.plist";

pub fn is_macos_15_or_later() -> bool {
    static MACOS_MAJOR_VERSION: OnceCell<u64> = OnceCell::new();
    let maybe_major_version = MACOS_MAJOR_VERSION
        .get_or_try_init(|| {
            // NOTE(cole-h): Sometimes, macOS decides it's a good idea to change the contents of the file you're reading.
            // See also:
            // https://eclecticlight.co/2020/08/13/macos-version-numbering-isnt-so-simple/
            // https://github.com/ziglang/zig/pull/7714/
            let symlink_path = std::path::Path::new(MACOS_SYSTEM_VERSION_PLIST_SYMLINK_PATH);
            let plist: SystemVersionPlist = if symlink_path.exists() {
                plist::from_file(symlink_path).with_context(|| {
                    format!("Failed to parse plist from {MACOS_SYSTEM_VERSION_PLIST_SYMLINK_PATH}")
                })?
            } else {
                plist::from_file(MACOS_SYSTEM_VERSION_PLIST_PATH).with_context(|| {
                    format!("Failed to parse plist from {MACOS_SYSTEM_VERSION_PLIST_PATH}")
                })?
            };

            let Some((major, _rest)) = plist.product_version.split_once('.') else {
                return Err(eyre::eyre!(
                    "Failed to parse ProductVersion: {}",
                    plist.product_version
                ));
            };

            let major = major
                .parse::<u64>()
                .with_context(|| format!("Failed to parse major version '{major}'"))?;

            Ok::<_, eyre::Error>(major)
        })
        .inspect_err(|e| {
            // NOTE(cole-h): cannot using tracing here because this is called before we setup the
            // tracing subscriber
            eprintln!(
                "{}",
                format!("WARNING: Failed to detect macOS major version, assuming <= macOS 14: {e}")
                    .yellow()
            );
        })
        .ok();

    maybe_major_version.is_some_and(|&v| v >= 15)
}

fn default_nix_build_user_id_base() -> u32 {
    use target_lexicon::OperatingSystem;

    match OperatingSystem::host() {
        OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => {
            // NOTE(cole-h): https://github.com/NixOS/nix/issues/10892#issuecomment-2212094287
            if is_macos_15_or_later() {
                450
            } else {
                300
            }
        },
        _ => 30_000,
    }
}

impl CommonSettings {
    /// The default settings for the given Architecture & Operating System
    pub async fn default() -> Result<Self, InstallSettingsError> {
        let nix_build_user_prefix;

        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            (Architecture::X86_64, OperatingSystem::Linux) => {
                nix_build_user_prefix = "nixbld";
            },
            (Architecture::X86_32(_), OperatingSystem::Linux) => {
                nix_build_user_prefix = "nixbld";
            },
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                nix_build_user_prefix = "nixbld";
            },
            (Architecture::X86_64, OperatingSystem::MacOSX { .. })
            | (Architecture::X86_64, OperatingSystem::Darwin) => {
                nix_build_user_prefix = "_nixbld";
            },
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. })
            | (Architecture::Aarch64(_), OperatingSystem::Darwin) => {
                nix_build_user_prefix = "_nixbld";
            },
            _ => {
                return Err(InstallSettingsError::UnsupportedArchitecture(
                    target_lexicon::HOST,
                ))
            },
        };

        Ok(Self {
            enterprise_edition: false,
            modify_profile: true,
            nix_build_group_name: String::from("nixbld"),
            nix_build_group_id: 30_000,
            nix_build_user_id_base: default_nix_build_user_id_base(),
            nix_build_user_count: 32,
            nix_build_user_prefix: nix_build_user_prefix.to_string(),
            nix_package_url: None,
            proxy: Default::default(),
            extra_conf: Default::default(),
            force: false,
            ssl_cert_file: Default::default(),
            #[cfg(feature = "diagnostics")]
            diagnostic_attribution: None,
            #[cfg(feature = "diagnostics")]
            diagnostic_endpoint: Some("https://install.determinate.systems/nix/diagnostic".into()),
        })
    }

    /// A listing of the settings, suitable for [`Planner::settings`](crate::planner::Planner::settings)
    pub fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self {
            enterprise_edition,
            modify_profile,
            nix_build_group_name,
            nix_build_group_id,
            nix_build_user_prefix,
            nix_build_user_id_base,
            nix_build_user_count,
            nix_package_url,
            proxy,
            extra_conf,
            force,
            ssl_cert_file,
            #[cfg(feature = "diagnostics")]
                diagnostic_attribution: _,
            #[cfg(feature = "diagnostics")]
            diagnostic_endpoint,
        } = self;
        let mut map = HashMap::default();

        map.insert(
            "enterprise_edition".into(),
            serde_json::to_value(enterprise_edition)?,
        );
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
            "nix_build_user_prefix".into(),
            serde_json::to_value(nix_build_user_prefix)?,
        );
        map.insert(
            "nix_build_user_id_base".into(),
            serde_json::to_value(nix_build_user_id_base)?,
        );
        map.insert(
            "nix_build_user_count".into(),
            serde_json::to_value(nix_build_user_count)?,
        );
        map.insert(
            "nix_package_url".into(),
            serde_json::to_value(nix_package_url)?,
        );
        map.insert("proxy".into(), serde_json::to_value(proxy)?);
        map.insert("ssl_cert_file".into(), serde_json::to_value(ssl_cert_file)?);
        map.insert("extra_conf".into(), serde_json::to_value(extra_conf)?);
        map.insert("force".into(), serde_json::to_value(force)?);

        #[cfg(feature = "diagnostics")]
        map.insert(
            "diagnostic_endpoint".into(),
            serde_json::to_value(diagnostic_endpoint)?,
        );

        Ok(map)
    }
}

async fn linux_detect_systemd_started() -> bool {
    use std::process::Stdio;

    let mut started = false;
    if std::path::Path::new("/run/systemd/system").exists() {
        started = tokio::process::Command::new("systemctl")
            .arg("status")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .ok()
            .map(|exit| exit.success())
            .unwrap_or(false)
    }

    // TODO: Other inits
    started
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
    pub init: InitSystem,

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
    pub start_daemon: bool,
}

impl InitSettings {
    /// The default settings for the given Architecture & Operating System
    pub async fn default() -> Result<Self, InstallSettingsError> {
        use target_lexicon::{Architecture, OperatingSystem};
        let (init, start_daemon) = match (Architecture::host(), OperatingSystem::host()) {
            (Architecture::X86_64, OperatingSystem::Linux) => {
                (InitSystem::Systemd, linux_detect_systemd_started().await)
            },
            (Architecture::X86_32(_), OperatingSystem::Linux) => {
                (InitSystem::Systemd, linux_detect_systemd_started().await)
            },
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                (InitSystem::Systemd, linux_detect_systemd_started().await)
            },
            (Architecture::X86_64, OperatingSystem::MacOSX { .. })
            | (Architecture::X86_64, OperatingSystem::Darwin) => (InitSystem::Launchd, true),
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. })
            | (Architecture::Aarch64(_), OperatingSystem::Darwin) => (InitSystem::Launchd, true),
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
#[non_exhaustive]
#[derive(thiserror::Error, Debug, strum::IntoStaticStr)]
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
    #[error(transparent)]
    UrlOrPath(#[from] UrlOrPathError),
}

#[derive(Debug, thiserror::Error)]
pub enum UrlOrPathError {
    #[error("Error parsing URL `{0}`")]
    Url(String, #[source] url::ParseError),
    #[error("The specified path `{0}` does not exist")]
    PathDoesNotExist(PathBuf),
    #[error("Error fetching URL `{0}`")]
    Reqwest(Url, #[source] reqwest::Error),
    #[error("I/O error when accessing `{0}`")]
    Io(PathBuf, #[source] std::io::Error),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize, Clone)]
pub enum UrlOrPath {
    Url(Url),
    Path(PathBuf),
}

impl Display for UrlOrPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UrlOrPath::Url(url) => f.write_fmt(format_args!("{url}")),
            UrlOrPath::Path(path) => f.write_fmt(format_args!("{}", path.display())),
        }
    }
}

impl FromStr for UrlOrPath {
    type Err = UrlOrPathError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Url::parse(s) {
            Ok(url) => Ok(UrlOrPath::Url(url)),
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                // This is most likely a relative path (`./boop` or `boop`)
                // or an absolute path (`/boop`)
                //
                // So we'll see if such a path exists, and if so, use it
                let path = PathBuf::from(s);
                if path.exists() {
                    Ok(UrlOrPath::Path(path))
                } else {
                    Err(UrlOrPathError::PathDoesNotExist(path))
                }
            },
            Err(e) => Err(UrlOrPathError::Url(s.to_string(), e)),
        }
    }
}

#[cfg(feature = "cli")]
impl clap::builder::TypedValueParser for UrlOrPath {
    type Value = UrlOrPath;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let value_str = value.to_str().ok_or_else(|| {
            let mut err = clap::Error::new(clap::error::ErrorKind::InvalidValue);
            err.insert(
                ContextKind::InvalidValue,
                ContextValue::String(format!("`{value:?}` not a UTF-8 string")),
            );
            err
        })?;
        match UrlOrPath::from_str(value_str) {
            Ok(v) => Ok(v),
            Err(from_str_error) => {
                let mut err = clap::Error::new(clap::error::ErrorKind::InvalidValue).with_cmd(cmd);
                err.insert(
                    clap::error::ContextKind::Custom,
                    clap::error::ContextValue::String(from_str_error.to_string()),
                );
                Err(err)
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize, Clone)]
pub enum UrlOrPathOrString {
    Url(Url),
    Path(PathBuf),
    String(String),
}

impl FromStr for UrlOrPathOrString {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Url::parse(s) {
            Ok(url) => Ok(UrlOrPathOrString::Url(url)),
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                // This is most likely a relative path (`./boop` or `boop`)
                // or an absolute path (`/boop`)
                //
                // So we'll see if such a path exists, and if so, use it
                let path = PathBuf::from(s);
                if path.exists() {
                    Ok(UrlOrPathOrString::Path(path))
                } else {
                    // The path doesn't exist, so the user is providing us with a string
                    Ok(UrlOrPathOrString::String(s.into()))
                }
            },
            Err(e) => Err(e),
        }
    }
}

#[cfg(feature = "cli")]
impl clap::builder::TypedValueParser for UrlOrPathOrString {
    type Value = UrlOrPathOrString;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        _arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let value_str = value.to_str().ok_or_else(|| {
            let mut err = clap::Error::new(clap::error::ErrorKind::InvalidValue);
            err.insert(
                ContextKind::InvalidValue,
                ContextValue::String(format!("`{value:?}` not a UTF-8 string")),
            );
            err
        })?;
        match UrlOrPathOrString::from_str(value_str) {
            Ok(v) => Ok(v),
            Err(from_str_error) => {
                let mut err = clap::Error::new(clap::error::ErrorKind::InvalidValue).with_cmd(cmd);
                err.insert(
                    clap::error::ContextKind::Custom,
                    clap::error::ContextValue::String(from_str_error.to_string()),
                );
                Err(err)
            },
        }
    }
}

#[cfg(feature = "diagnostics")]
impl crate::diagnostics::ErrorDiagnostic for InstallSettingsError {
    fn diagnostic(&self) -> String {
        let static_str: &'static str = (self).into();
        static_str.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{FromStr, PathBuf, Url, UrlOrPath, UrlOrPathOrString};

    #[test]
    fn url_or_path_or_string_parses() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            UrlOrPathOrString::from_str("https://boop.bleat")?,
            UrlOrPathOrString::Url(Url::from_str("https://boop.bleat")?),
        );
        assert_eq!(
            UrlOrPathOrString::from_str("file:///boop/bleat")?,
            UrlOrPathOrString::Url(Url::from_str("file:///boop/bleat")?),
        );
        // The file *must* exist!
        assert_eq!(
            UrlOrPathOrString::from_str(file!())?,
            UrlOrPathOrString::Path(PathBuf::from_str(file!())?),
        );
        assert_eq!(
            UrlOrPathOrString::from_str("Boop")?,
            UrlOrPathOrString::String(String::from("Boop")),
        );
        Ok(())
    }

    #[test]
    fn url_or_path_parses() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            UrlOrPath::from_str("https://boop.bleat")?,
            UrlOrPath::Url(Url::from_str("https://boop.bleat")?),
        );
        assert_eq!(
            UrlOrPath::from_str("file:///boop/bleat")?,
            UrlOrPath::Url(Url::from_str("file:///boop/bleat")?),
        );
        // The file *must* exist!
        assert_eq!(
            UrlOrPath::from_str(file!())?,
            UrlOrPath::Path(PathBuf::from_str(file!())?),
        );
        Ok(())
    }
}
