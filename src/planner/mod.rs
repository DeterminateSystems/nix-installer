/*! [`BuiltinPlanner`]s and traits to create new types which can be used to plan out an [`InstallPlan`]

It's a [`Planner`]s job to construct (if possible) a valid [`InstallPlan`] for the host. Some planners are operating system specific, others are device specific.

[`Planner`]s contain their planner specific settings, typically alongside a [`CommonSettings`][crate::settings::CommonSettings].

[`BuiltinPlanner::default()`] offers a way to get the default builtin planner for a given host.

Custom Planners can also be used to create a platform, project, or organization specific install.

A custom [`Planner`] can be created:

```rust,no_run
use std::{error::Error, collections::HashMap};
use nix_installer::{
    InstallPlan,
    settings::{CommonSettings, InstallSettingsError},
    planner::{Planner, PlannerError},
    action::{Action, StatefulAction, base::CreateFile},
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MyPlanner {
    pub common: CommonSettings,
}


#[async_trait::async_trait]
#[typetag::serde(name = "my-planner")]
impl Planner for MyPlanner {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            common: CommonSettings::default().await?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        Ok(vec![
            // ...

                CreateFile::plan("/example", None, None, None, "Example".to_string(), false)
                    .await
                    .map_err(PlannerError::Action)?.boxed(),
        ])
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self { common } = self;
        let mut map = std::collections::HashMap::default();

        map.extend(common.settings()?.into_iter());

        Ok(map)
    }

    async fn configured_settings(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, PlannerError> {
        let default = Self::default().await?.settings()?;
        let configured = self.settings()?;

        let mut settings: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, value) in configured.iter() {
            if default.get(key) != Some(value) {
                settings.insert(key.clone(), value.clone());
            }
        }

        Ok(settings)
    }

    #[cfg(feature = "diagnostics")]
    async fn diagnostic_data(&self) -> Result<nix_installer::diagnostics::DiagnosticData, PlannerError> {
        Ok(nix_installer::diagnostics::DiagnosticData::new(
            self.common.diagnostic_attribution.clone(),
            self.common.diagnostic_endpoint.clone(),
            self.typetag_name().into(),
            self.configured_settings()
                .await?
                .into_keys()
                .collect::<Vec<_>>(),
            self.common.ssl_cert_file.clone(),
        )?)
    }
}

# async fn custom_planner_install() -> color_eyre::Result<()> {
let planner = MyPlanner::default().await?;
let mut plan = InstallPlan::plan(planner).await?;
match plan.install(None).await {
    Ok(()) => tracing::info!("Done"),
    Err(e) => {
        match e.source() {
            Some(source) => tracing::error!("{e}: {}", source),
            None => tracing::error!("{e}"),
        };
        plan.uninstall(None).await?;
    },
};

#    Ok(())
# }
```

*/
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub mod ostree;
#[cfg(target_os = "linux")]
pub mod steam_deck;

use std::{collections::HashMap, path::PathBuf, string::FromUtf8Error};

use serde::{Deserialize, Serialize};

use crate::{
    action::{ActionError, StatefulAction},
    error::HasExpectedErrors,
    settings::{CommonSettings, InstallSettingsError},
    Action, InstallPlan, NixInstallerError,
};

/// Something which can be used to plan out an [`InstallPlan`]
#[async_trait::async_trait]
#[typetag::serde(tag = "planner")]
pub trait Planner: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    /// Instantiate the planner with default settings, if possible
    async fn default() -> Result<Self, PlannerError>
    where
        Self: Sized;
    /// Plan out the [`Action`]s for an [`InstallPlan`]
    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError>;
    /// The settings being used by the planner
    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError>;

    async fn configured_settings(&self)
        -> Result<HashMap<String, serde_json::Value>, PlannerError>;

    /// A boxed, type erased planner
    fn boxed(self) -> Box<dyn Planner>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }

    async fn pre_uninstall_check(&self) -> Result<(), PlannerError> {
        Ok(())
    }

    async fn pre_install_check(&self) -> Result<(), PlannerError> {
        Ok(())
    }

    #[cfg(feature = "diagnostics")]
    async fn diagnostic_data(&self) -> Result<crate::diagnostics::DiagnosticData, PlannerError>;
}

dyn_clone::clone_trait_object!(Planner);

/// Planners built into this crate
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "cli", derive(clap::Subcommand))]
pub enum BuiltinPlanner {
    #[cfg(target_os = "linux")]
    /// A planner for traditional, mutable Linux systems like Debian, RHEL, or Arch
    Linux(linux::Linux),
    /// A planner for the Valve Steam Deck running SteamOS
    #[cfg(target_os = "linux")]
    SteamDeck(steam_deck::SteamDeck),
    /// A planner suitable for immutable systems using ostree, such as Fedora Silverblue
    #[cfg(target_os = "linux")]
    Ostree(ostree::Ostree),
    /// A planner for MacOS (Darwin) systems
    #[cfg(target_os = "macos")]
    Macos(macos::Macos),
}

impl BuiltinPlanner {
    /// Heuristically determine the default planner for the target system
    pub async fn default() -> Result<Self, PlannerError> {
        use target_lexicon::{Architecture, OperatingSystem};
        match (Architecture::host(), OperatingSystem::host()) {
            #[cfg(target_os = "linux")]
            (Architecture::X86_64, OperatingSystem::Linux) => Self::detect_linux_distro().await,
            #[cfg(target_os = "linux")]
            (Architecture::X86_32(_), OperatingSystem::Linux) => {
                Ok(Self::Linux(linux::Linux::default().await?))
            },
            #[cfg(target_os = "linux")]
            (Architecture::Aarch64(_), OperatingSystem::Linux) => {
                Ok(Self::Linux(linux::Linux::default().await?))
            },
            #[cfg(target_os = "macos")]
            (Architecture::X86_64, OperatingSystem::MacOSX { .. })
            | (Architecture::X86_64, OperatingSystem::Darwin) => {
                Ok(Self::Macos(macos::Macos::default().await?))
            },
            #[cfg(target_os = "macos")]
            (Architecture::Aarch64(_), OperatingSystem::MacOSX { .. })
            | (Architecture::Aarch64(_), OperatingSystem::Darwin) => {
                Ok(Self::Macos(macos::Macos::default().await?))
            },
            _ => Err(PlannerError::UnsupportedArchitecture(target_lexicon::HOST)),
        }
    }

    #[cfg(target_os = "linux")]
    async fn detect_linux_distro() -> Result<Self, PlannerError> {
        let is_steam_deck =
            os_release::OsRelease::new().is_ok_and(|os_release| os_release.id == "steamos");
        if is_steam_deck {
            return Ok(Self::SteamDeck(steam_deck::SteamDeck::default().await?));
        }

        let is_ostree = std::process::Command::new("ostree")
            .arg("remote")
            .arg("list")
            .output()
            .is_ok_and(|output| output.status.success());
        if is_ostree {
            return Ok(Self::Ostree(ostree::Ostree::default().await?));
        }

        Ok(Self::Linux(linux::Linux::default().await?))
    }

    pub async fn from_common_settings(settings: CommonSettings) -> Result<Self, PlannerError> {
        let mut built = Self::default().await?;
        match &mut built {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(inner) => inner.settings = settings,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(inner) => inner.settings = settings,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Ostree(inner) => inner.settings = settings,
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(inner) => inner.settings = settings,
        }
        Ok(built)
    }

    pub async fn configured_settings(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, PlannerError> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(inner) => inner.configured_settings().await,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(inner) => inner.configured_settings().await,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Ostree(inner) => inner.configured_settings().await,
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(inner) => inner.configured_settings().await,
        }
    }

    pub async fn plan(self) -> Result<InstallPlan, NixInstallerError> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(planner) => InstallPlan::plan(planner).await,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(planner) => InstallPlan::plan(planner).await,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Ostree(planner) => InstallPlan::plan(planner).await,
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(planner) => InstallPlan::plan(planner).await,
        }
    }
    pub fn boxed(self) -> Box<dyn Planner> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(i) => i.boxed(),
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(i) => i.boxed(),
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Ostree(i) => i.boxed(),
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(i) => i.boxed(),
        }
    }

    pub fn typetag_name(&self) -> &'static str {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(i) => i.typetag_name(),
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(i) => i.typetag_name(),
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Ostree(i) => i.typetag_name(),
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(i) => i.typetag_name(),
        }
    }

    pub fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(i) => i.settings(),
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(i) => i.settings(),
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Ostree(i) => i.settings(),
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(i) => i.settings(),
        }
    }

    #[cfg(feature = "diagnostics")]
    pub async fn diagnostic_data(
        &self,
    ) -> Result<crate::diagnostics::DiagnosticData, PlannerError> {
        match self {
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Linux(i) => i.diagnostic_data().await,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::SteamDeck(i) => i.diagnostic_data().await,
            #[cfg(target_os = "linux")]
            BuiltinPlanner::Ostree(i) => i.diagnostic_data().await,
            #[cfg(target_os = "macos")]
            BuiltinPlanner::Macos(i) => i.diagnostic_data().await,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct ShellProfileLocations {
    pub fish: FishShellProfileLocations,
    pub bash: Vec<PathBuf>,
    pub zsh: Vec<PathBuf>,
}

impl Default for ShellProfileLocations {
    fn default() -> Self {
        Self {
            fish: FishShellProfileLocations::default(),
            bash: vec![
                "/etc/bashrc".into(),
                "/etc/profile.d/nix.sh".into(),
                "/etc/bash.bashrc".into(),
            ],
            zsh: vec![
                // https://zsh.sourceforge.io/Intro/intro_3.html
                "/etc/zshrc".into(),
                "/etc/zsh/zshrc".into(),
            ],
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct FishShellProfileLocations {
    pub confd_suffix: PathBuf,
    /**
     Each of these are common values of $__fish_sysconf_dir,
    under which Fish will look for the file named by
    `confd_suffix`.
    */
    pub confd_prefixes: Vec<PathBuf>,
    /// Fish has different syntax than zsh/bash, treat it separate
    pub vendor_confd_suffix: PathBuf,
    /**
    Each of these are common values of $__fish_vendor_confdir,
    under which Fish will look for the file named by
    `confd_suffix`.

    More info: <https://fishshell.com/docs/3.3/index.html#configuration-files>
    */
    pub vendor_confd_prefixes: Vec<PathBuf>,
}

impl Default for FishShellProfileLocations {
    fn default() -> Self {
        Self {
            confd_prefixes: vec![
                "/etc/fish".into(),              // standard
                "/usr/local/etc/fish".into(),    // their installer .pkg for macOS
                "/opt/homebrew/etc/fish".into(), // homebrew
                "/opt/local/etc/fish".into(),    // macports
            ],
            confd_suffix: "conf.d/nix.fish".into(),
            vendor_confd_prefixes: vec!["/usr/share/fish/".into(), "/usr/local/share/fish/".into()],
            vendor_confd_suffix: "vendor_conf.d/nix.fish".into(),
        }
    }
}

/// An error originating from a [`Planner`]
#[non_exhaustive]
#[derive(thiserror::Error, Debug, strum::IntoStaticStr)]
pub enum PlannerError {
    /// `nix-installer` does not have a default planner for the target architecture right now
    #[error("`nix-installer` does not have a default planner for the `{0}` architecture right now, pass a specific archetype")]
    UnsupportedArchitecture(target_lexicon::Triple),
    /// Error executing action
    #[error("Error executing action")]
    Action(
        #[source]
        #[from]
        ActionError,
    ),
    /// An [`InstallSettingsError`]
    #[error(transparent)]
    InstallSettings(#[from] InstallSettingsError),
    /// An OS Release error
    #[error("Fetching `/etc/os-release`")]
    OsRelease(#[source] std::io::Error),
    /// A MacOS (Darwin) plist related error
    #[error(transparent)]
    Plist(#[from] plist::Error),
    #[error(transparent)]
    Sysctl(#[from] sysctl::SysctlError),
    #[error("Detected that this process is running under Rosetta, using Nix in Rosetta is not supported (Please open an issue with your use case)")]
    RosettaDetected,
    #[error("Nix Enterprise is not available. See: https://determinate.systems/enterprise")]
    NixEnterpriseUnavailable,
    /// A Linux SELinux related error
    #[error("Unable to install on an SELinux system without common SELinux tooling, the binaries `restorecon`, and `semodule` are required")]
    SelinuxRequirements,
    /// A UTF-8 related error
    #[error("UTF-8 error")]
    Utf8(#[from] FromUtf8Error),
    /// Custom planner error
    #[error("Custom planner error")]
    Custom(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("NixOS already has Nix installed")]
    NixOs,
    #[error("`nix` is already a valid command, so it is installed")]
    NixExists,
    #[error("WSL1 is not supported, please upgrade to WSL2: https://learn.microsoft.com/en-us/windows/wsl/install#upgrade-version-from-wsl-1-to-wsl-2")]
    Wsl1,
    /// Failed to execute command
    #[error("Failed to execute command `{0}`")]
    Command(String, #[source] std::io::Error),
    #[cfg(feature = "diagnostics")]
    #[error(transparent)]
    Diagnostic(#[from] crate::diagnostics::DiagnosticError),
}

impl HasExpectedErrors for PlannerError {
    fn expected<'a>(&'a self) -> Option<Box<dyn std::error::Error + 'a>> {
        match self {
            this @ PlannerError::UnsupportedArchitecture(_) => Some(Box::new(this)),
            PlannerError::Action(_) => None,
            PlannerError::InstallSettings(_) => None,
            PlannerError::Plist(_) => None,
            PlannerError::Sysctl(_) => None,
            this @ PlannerError::RosettaDetected => Some(Box::new(this)),
            this @ PlannerError::NixEnterpriseUnavailable => Some(Box::new(this)),
            PlannerError::OsRelease(_) => None,
            PlannerError::Utf8(_) => None,
            PlannerError::SelinuxRequirements => Some(Box::new(self)),
            PlannerError::Custom(_e) => {
                #[cfg(target_os = "linux")]
                if let Some(err) = _e.downcast_ref::<linux::LinuxErrorKind>() {
                    return err.expected();
                }
                #[cfg(target_os = "macos")]
                if let Some(err) = _e.downcast_ref::<macos::MacosError>() {
                    return err.expected();
                }
                None
            },
            this @ PlannerError::NixOs => Some(Box::new(this)),
            this @ PlannerError::NixExists => Some(Box::new(this)),
            this @ PlannerError::Wsl1 => Some(Box::new(this)),
            PlannerError::Command(_, _) => None,
            #[cfg(feature = "diagnostics")]
            PlannerError::Diagnostic(diagnostic_error) => Some(Box::new(diagnostic_error)),
        }
    }
}

#[cfg(feature = "diagnostics")]
impl crate::diagnostics::ErrorDiagnostic for PlannerError {
    fn diagnostic(&self) -> String {
        let static_str: &'static str = (self).into();
        static_str.to_string()
    }
}
