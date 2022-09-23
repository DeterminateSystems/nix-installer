pub mod base;
pub mod meta;

use std::fmt::Display;

use base::{
    ConfigureNixDaemonService, ConfigureNixDaemonServiceReceipt, CreateDirectory,
    CreateDirectoryReceipt, CreateFile, CreateFileReceipt, CreateGroup, CreateGroupReceipt,
    CreateOrAppendFile, CreateOrAppendFileReceipt, CreateUser, CreateUserReceipt, FetchNix,
    FetchNixReceipt, MoveUnpackedNix, MoveUnpackedNixReceipt, PlaceChannelConfiguration,
    PlaceChannelConfigurationReceipt, PlaceNixConfiguration, PlaceNixConfigurationReceipt,
    SetupDefaultProfile, SetupDefaultProfileReceipt,
};
use meta::{
    ConfigureNix, ConfigureNixReceipt, ConfigureShellProfile, ConfigureShellProfileReceipt,
    CreateNixTree, CreateNixTreeReceipt, CreateUsersAndGroup, CreateUsersAndGroupReceipt,
    ProvisionNix, ProvisionNixReceipt, StartNixDaemon, StartNixDaemonReceipt,
};

use crate::{HarmonicError, error::ActionState};

#[async_trait::async_trait]
pub trait Actionable: serde::de::DeserializeOwned + serde::Serialize {
    type Receipt: Revertable;
    type Error: std::error::Error + Display;
    fn description(&self) -> Vec<ActionDescription>;
    async fn execute(self) -> ActionState<Self>;
}

#[async_trait::async_trait]
pub trait Revertable: serde::de::DeserializeOwned + serde::Serialize {
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
    CreateFile(CreateFile),
    CreateGroup(CreateGroup),
    CreateOrAppendFile(CreateOrAppendFile),
    CreateNixTree(CreateNixTree),
    CreateUser(CreateUser),
    CreateUsersAndGroup(CreateUsersAndGroup),
    FetchNix(FetchNix),
    MoveUnpackedNix(MoveUnpackedNix),
    PlaceChannelConfiguration(PlaceChannelConfiguration),
    PlaceNixConfiguration(PlaceNixConfiguration),
    SetupDefaultProfile(SetupDefaultProfile),
    StartNixDaemon(StartNixDaemon),
    ProvisionNix(ProvisionNix),
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub enum ActionReceipt {
    ConfigureNixDaemonService(ConfigureNixDaemonServiceReceipt),
    ConfigureNix(ConfigureNixReceipt),
    ConfigureShellProfile(ConfigureShellProfileReceipt),
    CreateDirectory(CreateDirectoryReceipt),
    CreateFile(CreateFileReceipt),
    CreateGroup(CreateGroupReceipt),
    CreateOrAppendFile(CreateOrAppendFileReceipt),
    CreateNixTree(CreateNixTreeReceipt),
    CreateUser(CreateUserReceipt),
    CreateUsersAndGroup(CreateUsersAndGroupReceipt),
    FetchNix(FetchNixReceipt),
    MoveUnpackedNix(MoveUnpackedNixReceipt),
    PlaceChannelConfiguration(PlaceChannelConfigurationReceipt),
    PlaceNixConfiguration(PlaceNixConfigurationReceipt),
    SetupDefaultProfile(SetupDefaultProfileReceipt),
    StartNixDaemon(StartNixDaemonReceipt),
    ProvisionNix(ProvisionNixReceipt),
}

#[derive(thiserror::Error, Debug)]
pub enum ActionError {

}

#[async_trait::async_trait]
impl Actionable for Action {
    type Receipt = ActionReceipt;
    type Error = ActionError;
    fn description(&self) -> Vec<ActionDescription> {
        match self {
            Action::ConfigureNixDaemonService(i) => i.description(),
            Action::ConfigureNix(i) => i.description(),
            Action::ConfigureShellProfile(i) => i.description(),
            Action::CreateDirectory(i) => i.description(),
            Action::CreateFile(i) => i.description(),
            Action::CreateGroup(i) => i.description(),
            Action::CreateOrAppendFile(i) => i.description(),
            Action::CreateNixTree(i) => i.description(),
            Action::CreateUser(i) => i.description(),
            Action::CreateUsersAndGroup(i) => i.description(),
            Action::FetchNix(i) => i.description(),
            Action::MoveUnpackedNix(i) => i.description(),
            Action::PlaceChannelConfiguration(i) => i.description(),
            Action::PlaceNixConfiguration(i) => i.description(),
            Action::SetupDefaultProfile(i) => i.description(),
            Action::StartNixDaemon(i) => i.description(),
            Action::ProvisionNix(i) => i.description(),
        }
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        match self {
            Action::ConfigureNixDaemonService(i) => i
                .execute()
                .await
                .map(ActionReceipt::ConfigureNixDaemonService),
            Action::ConfigureNix(i) => i.execute().await.map(ActionReceipt::ConfigureNix),
            Action::ConfigureShellProfile(i) => {
                i.execute().await.map(ActionReceipt::ConfigureShellProfile)
            },
            Action::CreateDirectory(i) => i.execute().await.map(ActionReceipt::CreateDirectory),
            Action::CreateFile(i) => i.execute().await.map(ActionReceipt::CreateFile),
            Action::CreateGroup(i) => i.execute().await.map(ActionReceipt::CreateGroup),
            Action::CreateOrAppendFile(i) => {
                i.execute().await.map(ActionReceipt::CreateOrAppendFile)
            },
            Action::CreateNixTree(i) => i.execute().await.map(ActionReceipt::CreateNixTree),
            Action::CreateUser(i) => i.execute().await.map(ActionReceipt::CreateUser),
            Action::CreateUsersAndGroup(i) => {
                i.execute().await.map(ActionReceipt::CreateUsersAndGroup)
            },
            Action::FetchNix(i) => i.execute().await.map(ActionReceipt::FetchNix),
            Action::MoveUnpackedNix(i) => i.execute().await.map(ActionReceipt::MoveUnpackedNix),
            Action::PlaceChannelConfiguration(i) => i
                .execute()
                .await
                .map(ActionReceipt::PlaceChannelConfiguration),
            Action::PlaceNixConfiguration(i) => {
                i.execute().await.map(ActionReceipt::PlaceNixConfiguration)
            },
            Action::SetupDefaultProfile(i) => {
                i.execute().await.map(ActionReceipt::SetupDefaultProfile)
            },
            Action::StartNixDaemon(i) => i.execute().await.map(ActionReceipt::StartNixDaemon),
            Action::ProvisionNix(i) => i.execute().await.map(ActionReceipt::ProvisionNix),
        }
    }
}

#[async_trait::async_trait]
impl Revertable for ActionReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        match self {
            ActionReceipt::ConfigureNixDaemonService(i) => i.description(),
            ActionReceipt::ConfigureNix(i) => i.description(),
            ActionReceipt::ConfigureShellProfile(i) => i.description(),
            ActionReceipt::CreateDirectory(i) => i.description(),
            ActionReceipt::CreateFile(i) => i.description(),
            ActionReceipt::CreateGroup(i) => i.description(),
            ActionReceipt::CreateOrAppendFile(i) => i.description(),
            ActionReceipt::CreateNixTree(i) => i.description(),
            ActionReceipt::CreateUser(i) => i.description(),
            ActionReceipt::CreateUsersAndGroup(i) => i.description(),
            ActionReceipt::FetchNix(i) => i.description(),
            ActionReceipt::MoveUnpackedNix(i) => i.description(),
            ActionReceipt::PlaceChannelConfiguration(i) => i.description(),
            ActionReceipt::PlaceNixConfiguration(i) => i.description(),
            ActionReceipt::SetupDefaultProfile(i) => i.description(),
            ActionReceipt::StartNixDaemon(i) => i.description(),
            ActionReceipt::ProvisionNix(i) => i.description(),
        }
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        match self {
            ActionReceipt::ConfigureNixDaemonService(i) => i.revert().await,
            ActionReceipt::ConfigureNix(i) => i.revert().await,
            ActionReceipt::ConfigureShellProfile(i) => i.revert().await,
            ActionReceipt::CreateDirectory(i) => i.revert().await,
            ActionReceipt::CreateFile(i) => i.revert().await,
            ActionReceipt::CreateGroup(i) => i.revert().await,
            ActionReceipt::CreateOrAppendFile(i) => i.revert().await,
            ActionReceipt::CreateNixTree(i) => i.revert().await,
            ActionReceipt::CreateUser(i) => i.revert().await,
            ActionReceipt::CreateUsersAndGroup(i) => i.revert().await,
            ActionReceipt::FetchNix(i) => i.revert().await,
            ActionReceipt::MoveUnpackedNix(i) => i.revert().await,
            ActionReceipt::PlaceChannelConfiguration(i) => i.revert().await,
            ActionReceipt::PlaceNixConfiguration(i) => i.revert().await,
            ActionReceipt::SetupDefaultProfile(i) => i.revert().await,
            ActionReceipt::StartNixDaemon(i) => i.revert().await,
            ActionReceipt::ProvisionNix(i) => i.revert().await,
        }
    }
}
