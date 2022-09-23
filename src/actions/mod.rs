pub mod base;
pub mod meta;

use std::{error::Error, fmt::Display};

use base::{
    ConfigureNixDaemonService, ConfigureNixDaemonServiceError, CreateDirectory,
    CreateDirectoryError, CreateFile, CreateFileError, CreateGroup, CreateGroupError,
    CreateOrAppendFile, CreateOrAppendFileError, CreateUser, CreateUserError, FetchNix,
    FetchNixError, MoveUnpackedNix, MoveUnpackedNixError, PlaceChannelConfiguration,
    PlaceChannelConfigurationError, PlaceNixConfiguration, PlaceNixConfigurationError,
    SetupDefaultProfile, SetupDefaultProfileError,
};
use meta::{
    ConfigureNix, ConfigureNixError, ConfigureShellProfile, ConfigureShellProfileError,
    CreateNixTree, CreateNixTreeError, CreateUsersAndGroup, CreateUsersAndGroupError,
    ProvisionNix, ProvisionNixError, StartNixDaemon, StartNixDaemonError,
};
use serde::{Deserialize, de::DeserializeOwned, Serialize};


use self::base::{StartSystemdUnit, StartSystemdUnitError};

#[async_trait::async_trait]
pub trait Actionable: DeserializeOwned + Serialize + Into<Action> {
    type Error: std::error::Error + std::fmt::Debug + Serialize + Into<ActionError>;

    fn description(&self) -> Vec<ActionDescription>;
    
    // They should also have an `async fn plan(args...) -> Result<ActionState<Self>, Self::Error>;`
    async fn execute(&mut self) -> Result<(), Self::Error>;
    async fn revert(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ActionState {
    Completed,
    Planned,
    Reverted,
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
    StartSystemdUnit(StartSystemdUnit),
    ProvisionNix(ProvisionNix),
}

#[derive(Debug, thiserror::Error, serde::Serialize)]
pub enum ActionError {
    #[error("Attempted to revert an unexecuted action")]
    NotExecuted(Action),
    #[error("Attempted to execute an already executed action")]
    AlreadyExecuted(Action),
    #[error("Attempted to revert an already reverted action")]
    AlreadyReverted(Action),
    #[error(transparent)]
    ConfigureNixDaemonService(#[from] ConfigureNixDaemonServiceError),
    #[error(transparent)]
    ConfigureNix(#[from] ConfigureNixError),
    #[error(transparent)]
    ConfigureShellProfile(#[from] ConfigureShellProfileError),
    #[error(transparent)]
    CreateDirectory(#[from] CreateDirectoryError),
    #[error(transparent)]
    CreateFile(#[from] CreateFileError),
    #[error(transparent)]
    CreateGroup(#[from] CreateGroupError),
    #[error(transparent)]
    CreateOrAppendFile(#[from] CreateOrAppendFileError),
    #[error(transparent)]
    CreateNixTree(#[from] CreateNixTreeError),
    #[error(transparent)]
    CreateUser(#[from] CreateUserError),
    #[error(transparent)]
    CreateUsersAndGroup(#[from] CreateUsersAndGroupError),
    #[error(transparent)]
    FetchNix(#[from] FetchNixError),
    #[error(transparent)]
    MoveUnpackedNix(#[from] MoveUnpackedNixError),
    #[error(transparent)]
    PlaceChannelConfiguration(#[from] PlaceChannelConfigurationError),
    #[error(transparent)]
    PlaceNixConfiguration(#[from] PlaceNixConfigurationError),
    #[error(transparent)]
    SetupDefaultProfile(#[from] SetupDefaultProfileError),
    #[error(transparent)]
    StartNixDaemon(#[from] StartNixDaemonError),
    #[error(transparent)]
    StartSystemdUnit(#[from] StartSystemdUnitError),
    #[error(transparent)]
    ProvisionNix(#[from] ProvisionNixError),
}

#[async_trait::async_trait]
impl Actionable for Action {
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
            Action::StartSystemdUnit(i) => i.description(),
            Action::ProvisionNix(i) => i.description(),
        }
    }

    async fn execute(&mut self) -> Result<(), Self::Error> {
        match self {
            Action::ConfigureNixDaemonService(i) => i.execute().await?,
            Action::ConfigureNix(i) => i.execute().await?,
            Action::ConfigureShellProfile(i) => i.execute().await?,
            Action::CreateDirectory(i) => i.execute().await?,
            Action::CreateFile(i) => i.execute().await?,
            Action::CreateGroup(i) => i.execute().await?,
            Action::CreateOrAppendFile(i) => i.execute().await?,
            Action::CreateNixTree(i) => i.execute().await?,
            Action::CreateUser(i) => i.execute().await?,
            Action::CreateUsersAndGroup(i) => i.execute().await?,
            Action::FetchNix(i) => i.execute().await?,
            Action::MoveUnpackedNix(i) => i.execute().await?,
            Action::PlaceChannelConfiguration(i) => i.execute().await?,
            Action::PlaceNixConfiguration(i) => i.execute().await?,
            Action::SetupDefaultProfile(i) => i.execute().await?,
            Action::StartNixDaemon(i) => i.execute().await?,
            Action::StartSystemdUnit(i) => i.execute().await?,
            Action::ProvisionNix(i) => i.execute().await?,
        };
        Ok(())
    }

    async fn revert(&mut self) -> Result<(), Self::Error> {
        match self {
            Action::ConfigureNixDaemonService(i) => i.revert().await?,
            Action::ConfigureNix(i) => i.revert().await?,
            Action::ConfigureShellProfile(i) => i.revert().await?,
            Action::CreateDirectory(i) => i.revert().await?,
            Action::CreateFile(i) => i.revert().await?,
            Action::CreateGroup(i) => i.revert().await?,
            Action::CreateOrAppendFile(i) => i.revert().await?,
            Action::CreateNixTree(i) => i.revert().await?,
            Action::CreateUser(i) => i.revert().await?,
            Action::CreateUsersAndGroup(i) => i.revert().await?,
            Action::FetchNix(i) => i.revert().await?,
            Action::MoveUnpackedNix(i) => i.revert().await?,
            Action::PlaceChannelConfiguration(i) => i.revert().await?,
            Action::PlaceNixConfiguration(i) => i.revert().await?,
            Action::SetupDefaultProfile(i) => i.revert().await?,
            Action::StartNixDaemon(i) => i.revert().await?,
            Action::StartSystemdUnit(i) => i.revert().await?,
            Action::ProvisionNix(i) => i.revert().await?,
        }
        Ok(())
    }
}
