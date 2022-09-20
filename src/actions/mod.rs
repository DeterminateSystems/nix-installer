pub mod base;
pub mod meta;

use base::{
    ConfigureNixDaemonService, ConfigureNixDaemonServiceReceipt,
    ConfigureShellProfile, ConfigureShellProfileReceipt,
    CreateDirectory, CreateDirectoryReceipt,
    CreateGroup, CreateGroupReceipt,
    CreateUser, CreateUserReceipt,
    FetchNix, FetchNixReceipt,
    MoveUnpackedNix, MoveUnpackedNixReceipt,
    PlaceChannelConfiguration, PlaceChannelConfigurationReceipt,
    PlaceNixConfiguration, PlaceNixConfigurationReceipt,
    SetupDefaultProfile, SetupDefaultProfileReceipt,
    StartSystemdService, StartSystemdServiceReceipt,
};
use meta::{
    ConfigureNix, ConfigureNixReceipt,
    CreateNixTree, CreateNixTreeReceipt,
    CreateNixTreeDirs, CreateNixTreeDirsReceipt,
    CreateUsersAndGroup, CreateUsersAndGroupReceipt,
    StartNixDaemon, StartNixDaemonReceipt,
};


use crate::HarmonicError;

use self::meta::{ProvisionNix, ProvisionNixReceipt};

#[async_trait::async_trait]
pub trait Actionable<'a>: serde::de::Deserialize<'a> + serde::Serialize {
    type Receipt;
    fn description(&self) -> Vec<ActionDescription>;
    async fn execute(self) -> Result<Self::Receipt, HarmonicError>;
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
    CreateDirectory(CreateDirectory),
    CreateGroup(CreateGroup),
    CreateNixTreeDirs(CreateNixTreeDirs),
    CreateNixTree(CreateNixTree),
    CreateUser(CreateUser),
    CreateUsersAndGroup(CreateUsersAndGroup),
    FetchNix(FetchNix),
    MoveUnpackedNix(MoveUnpackedNix),
    PlaceChannelConfiguration(PlaceChannelConfiguration),
    PlaceNixConfiguration(PlaceNixConfiguration),
    SetupDefaultProfile(SetupDefaultProfile),
    StartNixDaemon(StartNixDaemon),
    StartSystemdService(StartSystemdService),
    ProvisionNix(ProvisionNix),
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
    CreateUsersAndGroup(CreateUsersAndGroupReceipt),
    FetchNix(FetchNixReceipt),
    MoveUnpackedNix(MoveUnpackedNixReceipt),
    PlaceChannelConfiguration(PlaceChannelConfigurationReceipt),
    PlaceNixConfiguration(PlaceNixConfigurationReceipt),
    SetupDefaultProfile(SetupDefaultProfileReceipt),
    StartNixDaemon(StartNixDaemonReceipt),
    StartSystemdService(StartSystemdServiceReceipt),
    ProvisionNix(ProvisionNixReceipt),
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for Action {
    type Receipt = ActionReceipt;
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
            Action::CreateUsersAndGroup(i) => i.description(),
            Action::FetchNix(i) => i.description(),
            Action::MoveUnpackedNix(i) => i.description(),
            Action::PlaceChannelConfiguration(i) => i.description(),
            Action::PlaceNixConfiguration(i) => i.description(),
            Action::SetupDefaultProfile(i) => i.description(),
            Action::StartNixDaemon(i) => i.description(),
            Action::StartSystemdService(i) => i.description(),
            Action::ProvisionNix(i) => i.description(),
        }
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        match self {
            Action::ConfigureNixDaemonService(i) => i.execute().await.map(ActionReceipt::ConfigureNixDaemonService),
            Action::ConfigureNix(i) => i.execute().await.map(ActionReceipt::ConfigureNix),
            Action::ConfigureShellProfile(i) => i.execute().await.map(ActionReceipt::ConfigureShellProfile),
            Action::CreateDirectory(i) => i.execute().await.map(ActionReceipt::CreateDirectory),
            Action::CreateGroup(i) => i.execute().await.map(ActionReceipt::CreateGroup),
            Action::CreateNixTreeDirs(i) => i.execute().await.map(ActionReceipt::CreateNixTreeDirs),
            Action::CreateNixTree(i) => i.execute().await.map(ActionReceipt::CreateNixTree),
            Action::CreateUser(i) => i.execute().await.map(ActionReceipt::CreateUser),
            Action::CreateUsersAndGroup(i) => i.execute().await.map(ActionReceipt::CreateUsersAndGroup),
            Action::FetchNix(i) => i.execute().await.map(ActionReceipt::FetchNix),
            Action::MoveUnpackedNix(i) => i.execute().await.map(ActionReceipt::MoveUnpackedNix),
            Action::PlaceChannelConfiguration(i) => i.execute().await.map(ActionReceipt::PlaceChannelConfiguration),
            Action::PlaceNixConfiguration(i) => i.execute().await.map(ActionReceipt::PlaceNixConfiguration),
            Action::SetupDefaultProfile(i) => i.execute().await.map(ActionReceipt::SetupDefaultProfile),
            Action::StartNixDaemon(i) => i.execute().await.map(ActionReceipt::StartNixDaemon),
            Action::StartSystemdService(i) => i.execute().await.map(ActionReceipt::StartSystemdService),
            Action::ProvisionNix(i) => i.execute().await.map(ActionReceipt::ProvisionNix),
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
            ActionReceipt::CreateUsersAndGroup(i) => i.description(),
            ActionReceipt::FetchNix(i) => i.description(),
            ActionReceipt::MoveUnpackedNix(i) => i.description(),
            ActionReceipt::PlaceChannelConfiguration(i) => i.description(),
            ActionReceipt::PlaceNixConfiguration(i) => i.description(),
            ActionReceipt::SetupDefaultProfile(i) => i.description(),
            ActionReceipt::StartNixDaemon(i) => i.description(),
            ActionReceipt::StartSystemdService(i) => i.description(),
            ActionReceipt::ProvisionNix(i) => i.description(),
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
            ActionReceipt::CreateUsersAndGroup(i) => i.revert().await,
            ActionReceipt::FetchNix(i) => i.revert().await,
            ActionReceipt::MoveUnpackedNix(i) => i.revert().await,
            ActionReceipt::PlaceChannelConfiguration(i) => i.revert().await,
            ActionReceipt::PlaceNixConfiguration(i) => i.revert().await,
            ActionReceipt::SetupDefaultProfile(i) => i.revert().await,
            ActionReceipt::StartNixDaemon(i) => i.revert().await,
            ActionReceipt::StartSystemdService(i) => i.revert().await,
            ActionReceipt::ProvisionNix(i) => i.revert().await,
        }
    }
}
