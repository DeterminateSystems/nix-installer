mod configure_nix;
mod configure_nix_daemon_service;
mod configure_shell_profile;
mod create_directory;
mod create_group;
mod create_nix_tree;
mod create_nix_tree_dirs;
mod create_user;
mod create_users;
mod place_channel_configuration;
mod place_nix_configuration;
mod setup_default_profile;
mod start_nix_daemon;
mod start_systemd_service;

pub use configure_nix::{ConfigureNix, ConfigureNixReceipt};
pub use configure_nix_daemon_service::{
    ConfigureNixDaemonService, ConfigureNixDaemonServiceReceipt,
};
pub use configure_shell_profile::{ConfigureShellProfile, ConfigureShellProfileReceipt};
pub use create_directory::{CreateDirectory, CreateDirectoryReceipt};
pub use create_group::{CreateGroup, CreateGroupReceipt};
pub use create_nix_tree::{CreateNixTree, CreateNixTreeReceipt};
pub use create_nix_tree_dirs::{CreateNixTreeDirs, CreateNixTreeDirsReceipt};
pub use create_user::{CreateUser, CreateUserReceipt};
pub use create_users::{CreateUsers, CreateUsersReceipt};
pub use place_channel_configuration::{
    PlaceChannelConfiguration, PlaceChannelConfigurationReceipt,
};
pub use place_nix_configuration::{PlaceNixConfiguration, PlaceNixConfigurationReceipt};
pub use setup_default_profile::{SetupDefaultProfile, SetupDefaultProfileReceipt};
pub use start_nix_daemon::{StartNixDaemon, StartNixDaemonReceipt};
pub use start_systemd_service::{StartSystemdService, StartSystemdServiceReceipt};

use crate::HarmonicError;

#[async_trait::async_trait]
pub trait Actionable<'a>: serde::de::Deserialize<'a> + serde::Serialize {
    fn description(&self) -> Vec<ActionDescription>;
    async fn execute(self) -> Result<ActionReceipt, HarmonicError>;
}

#[async_trait::async_trait]
pub trait Revertable<'a>: serde::de::Deserialize<'a> + serde::Serialize {
    fn description(&self) -> Vec<ActionDescription>;
    async fn revert(self) -> Result<(), HarmonicError>;
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]

pub struct ActionDescription {
    pub description: String,
    pub explanation: Vec<String>,
}

impl ActionDescription {
    fn new(description: String, explanation: Vec<String>) -> Self {
        Self {
            description,
            explanation,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub enum Action {
    ConfigureNixDaemonService(ConfigureNixDaemonService),
    ConfigureNix(ConfigureNix),
    ConfigureShellProfile(ConfigureShellProfile),
    CreateDirectory(CreateUser),
    CreateGroup(CreateGroup),
    CreateNixTreeDirs(CreateNixTreeDirs),
    CreateNixTree(CreateNixTree),
    CreateUser(CreateUser),
    CreateUsers(CreateUsers),
    PlaceChannelConfiguration(PlaceChannelConfiguration),
    PlaceNixConfiguration(PlaceNixConfiguration),
    SetupDefaultProfile(SetupDefaultProfile),
    StartNixDaemon(StartNixDaemon),
    StartSystemdService(StartNixDaemon),
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub enum ActionReceipt {
    ConfigureNixDaemonService(ConfigureNixDaemonServiceReceipt),
    ConfigureNix(ConfigureNixReceipt),
    ConfigureShellProfile(ConfigureShellProfileReceipt),
    CreateDirectory(CreateDirectoryReceipt),
    CreateGroup(CreateGroupReceipt),
    CreateNixTreeDirs(CreateNixTreeDirsReceipt),
    CreateNixTree(CreateNixTreeReceipt),
    CreateUser(CreateUserReceipt),
    CreateUsers(CreateUsersReceipt),
    PlaceChannelConfiguration(PlaceChannelConfigurationReceipt),
    PlaceNixConfiguration(PlaceNixConfigurationReceipt),
    SetupDefaultProfile(SetupDefaultProfileReceipt),
    StartNixDaemon(StartNixDaemonReceipt),
    StartSystemdService(StartNixDaemonReceipt),
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for Action {
    fn description(&self) -> Vec<ActionDescription> {
        match self {
            Action::ConfigureNixDaemonService(i) => i.description(),
            Action::ConfigureNix(i) => i.description(),
            Action::ConfigureShellProfile(i) => i.description(),
            Action::CreateDirectory(i) => i.description(),
            Action::CreateGroup(i) => i.description(),
            Action::CreateNixTreeDirs(i) => i.description(),
            Action::CreateNixTree(i) => i.description(),
            Action::CreateUser(i) => i.description(),
            Action::CreateUsers(i) => i.description(),
            Action::PlaceChannelConfiguration(i) => i.description(),
            Action::PlaceNixConfiguration(i) => i.description(),
            Action::SetupDefaultProfile(i) => i.description(),
            Action::StartNixDaemon(i) => i.description(),
            Action::StartSystemdService(i) => i.description(),
        }
    }

    async fn execute(self) -> Result<ActionReceipt, HarmonicError> {
        match self {
            Action::ConfigureNixDaemonService(i) => i.execute().await,
            Action::ConfigureNix(i) => i.execute().await,
            Action::ConfigureShellProfile(i) => i.execute().await,
            Action::CreateDirectory(i) => i.execute().await,
            Action::CreateGroup(i) => i.execute().await,
            Action::CreateNixTreeDirs(i) => i.execute().await,
            Action::CreateNixTree(i) => i.execute().await,
            Action::CreateUser(i) => i.execute().await,
            Action::CreateUsers(i) => i.execute().await,
            Action::PlaceChannelConfiguration(i) => i.execute().await,
            Action::PlaceNixConfiguration(i) => i.execute().await,
            Action::SetupDefaultProfile(i) => i.execute().await,
            Action::StartNixDaemon(i) => i.execute().await,
            Action::StartSystemdService(i) => i.execute().await,
        }
    }
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for ActionReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        match self {
            ActionReceipt::ConfigureNixDaemonService(i) => i.description(),
            ActionReceipt::ConfigureNix(i) => i.description(),
            ActionReceipt::ConfigureShellProfile(i) => i.description(),
            ActionReceipt::CreateDirectory(i) => i.description(),
            ActionReceipt::CreateGroup(i) => i.description(),
            ActionReceipt::CreateNixTreeDirs(i) => i.description(),
            ActionReceipt::CreateNixTree(i) => i.description(),
            ActionReceipt::CreateUser(i) => i.description(),
            ActionReceipt::CreateUsers(i) => i.description(),
            ActionReceipt::PlaceChannelConfiguration(i) => i.description(),
            ActionReceipt::PlaceNixConfiguration(i) => i.description(),
            ActionReceipt::SetupDefaultProfile(i) => i.description(),
            ActionReceipt::StartNixDaemon(i) => i.description(),
            ActionReceipt::StartSystemdService(i) => i.description(),
        }
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        match self {
            ActionReceipt::ConfigureNixDaemonService(i) => i.revert().await,
            ActionReceipt::ConfigureNix(i) => i.revert().await,
            ActionReceipt::ConfigureShellProfile(i) => i.revert().await,
            ActionReceipt::CreateDirectory(i) => i.revert().await,
            ActionReceipt::CreateGroup(i) => i.revert().await,
            ActionReceipt::CreateNixTreeDirs(i) => i.revert().await,
            ActionReceipt::CreateNixTree(i) => i.revert().await,
            ActionReceipt::CreateUser(i) => i.revert().await,
            ActionReceipt::CreateUsers(i) => i.revert().await,
            ActionReceipt::PlaceChannelConfiguration(i) => i.revert().await,
            ActionReceipt::PlaceNixConfiguration(i) => i.revert().await,
            ActionReceipt::SetupDefaultProfile(i) => i.revert().await,
            ActionReceipt::StartNixDaemon(i) => i.revert().await,
            ActionReceipt::StartSystemdService(i) => i.revert().await,
        }
    }
}
