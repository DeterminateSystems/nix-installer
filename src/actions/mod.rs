pub mod base;
pub mod meta;

use base::{
    ConfigureNixDaemonService, ConfigureNixDaemonServiceError, CreateDirectory,
    CreateDirectoryError, CreateFile, CreateFileError, CreateGroup, CreateGroupError,
    CreateOrAppendFile, CreateOrAppendFileError, CreateUser, CreateUserError, FetchNix,
    FetchNixError, MoveUnpackedNix, MoveUnpackedNixError, SetupDefaultProfile,
    SetupDefaultProfileError,
};
use futures::Future;
use meta::{
    ConfigureNix, ConfigureNixError, ConfigureShellProfile, ConfigureShellProfileError,
    CreateNixTree, CreateNixTreeError, CreateUsersAndGroup, CreateUsersAndGroupError,
    PlaceChannelConfiguration, PlaceChannelConfigurationError, PlaceNixConfiguration,
    PlaceNixConfigurationError, ProvisionNix, ProvisionNixError, StartNixDaemon,
    StartNixDaemonError,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use self::base::{StartSystemdUnit, StartSystemdUnitError};

#[async_trait::async_trait]
pub trait Actionable: DeserializeOwned + Serialize + Into<Action> {
    type Error: std::error::Error + std::fmt::Debug + Serialize + Into<ActionError>;

    fn describe_execute(&self) -> Vec<ActionDescription>;
    fn describe_revert(&self) -> Vec<ActionDescription>;

    // They should also have an `async fn plan(args...) -> Result<ActionState<Self>, Self::Error>;`
    async fn execute(&mut self) -> Result<(), Self::Error>;
    async fn revert(&mut self) -> Result<(), Self::Error>;
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ActionState {
    Completed,
    // Only applicable to meta-actions that start multiple sub-actions.
    Progress,
    Uncompleted,
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
    fn describe_execute(&self) -> Vec<ActionDescription> {
        match self {
            Action::ConfigureNixDaemonService(i) => i.describe_execute(),
            Action::ConfigureNix(i) => i.describe_execute(),
            Action::ConfigureShellProfile(i) => i.describe_execute(),
            Action::CreateDirectory(i) => i.describe_execute(),
            Action::CreateFile(i) => i.describe_execute(),
            Action::CreateGroup(i) => i.describe_execute(),
            Action::CreateOrAppendFile(i) => i.describe_execute(),
            Action::CreateNixTree(i) => i.describe_execute(),
            Action::CreateUser(i) => i.describe_execute(),
            Action::CreateUsersAndGroup(i) => i.describe_execute(),
            Action::FetchNix(i) => i.describe_execute(),
            Action::MoveUnpackedNix(i) => i.describe_execute(),
            Action::PlaceChannelConfiguration(i) => i.describe_execute(),
            Action::PlaceNixConfiguration(i) => i.describe_execute(),
            Action::SetupDefaultProfile(i) => i.describe_execute(),
            Action::StartNixDaemon(i) => i.describe_execute(),
            Action::StartSystemdUnit(i) => i.describe_execute(),
            Action::ProvisionNix(i) => i.describe_execute(),
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

    fn describe_revert(&self) -> Vec<ActionDescription> {
        match self {
            Action::ConfigureNixDaemonService(i) => i.describe_revert(),
            Action::ConfigureNix(i) => i.describe_revert(),
            Action::ConfigureShellProfile(i) => i.describe_revert(),
            Action::CreateDirectory(i) => i.describe_revert(),
            Action::CreateFile(i) => i.describe_revert(),
            Action::CreateGroup(i) => i.describe_revert(),
            Action::CreateOrAppendFile(i) => i.describe_revert(),
            Action::CreateNixTree(i) => i.describe_revert(),
            Action::CreateUser(i) => i.describe_revert(),
            Action::CreateUsersAndGroup(i) => i.describe_revert(),
            Action::FetchNix(i) => i.describe_revert(),
            Action::MoveUnpackedNix(i) => i.describe_revert(),
            Action::PlaceChannelConfiguration(i) => i.describe_revert(),
            Action::PlaceNixConfiguration(i) => i.describe_revert(),
            Action::SetupDefaultProfile(i) => i.describe_revert(),
            Action::StartNixDaemon(i) => i.describe_revert(),
            Action::StartSystemdUnit(i) => i.describe_revert(),
            Action::ProvisionNix(i) => i.describe_revert(),
        }
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
