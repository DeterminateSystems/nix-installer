mod start_nix_daemon_service;
mod create_users;
mod create_user;
pub use start_nix_daemon_service::{StartNixDaemonService, StartNixDaemonServiceReceipt};
pub use create_user::{CreateUser, CreateUserReceipt};
pub use create_users::{CreateUsers, CreateUsersReceipt};

use crate::{HarmonicError, settings::InstallSettings};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub enum Action {
    CreateUsers(CreateUsers),
    CreateUser(CreateUser),
    StartNixDaemonService(StartNixDaemonService),
}


#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub enum ActionReceipt {
    CreateUsers(CreateUsersReceipt),
    CreateUser(CreateUserReceipt),
    StartNixDaemonService(StartNixDaemonServiceReceipt), 
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for Action {
    fn description(&self) -> String {
        match self {
            Action::StartNixDaemonService(i) => i.description(),
            Action::CreateUser(i) => i.description(),
            Action::CreateUsers(i) => i.description(),
        }
    }

    async fn execute(self) -> Result<ActionReceipt, HarmonicError> {
        match self {
            Action::StartNixDaemonService(i) => i.execute().await,
            Action::CreateUser(i) => i.execute().await,
            Action::CreateUsers(i) => i.execute().await,
        }
    }
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for ActionReceipt {
    fn description(&self) -> String {
        match self {
            ActionReceipt::StartNixDaemonService(i) => i.description(),
            ActionReceipt::CreateUser(i) => i.description(),
            ActionReceipt::CreateUsers(i) => i.description(),
        }
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        match self {
            ActionReceipt::StartNixDaemonService(i) => i.revert().await,
            ActionReceipt::CreateUser(i) => i.revert().await,
            ActionReceipt::CreateUsers(i) => i.revert().await,
        }
    }
}

#[async_trait::async_trait]
pub trait Actionable<'a>: serde::de::Deserialize<'a> + serde::Serialize {
    fn description(&self) -> String;
    async fn execute(self) -> Result<ActionReceipt, HarmonicError>;
}

#[async_trait::async_trait]
pub trait Revertable<'a>: serde::de::Deserialize<'a> + serde::Serialize {
    fn description(&self) -> String;
    async fn revert(self) -> Result<(), HarmonicError>;
}
