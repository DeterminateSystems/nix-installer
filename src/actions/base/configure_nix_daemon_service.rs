use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::Command;

use crate::{execute_command, HarmonicError};

use crate::actions::{ActionDescription, Actionable, ActionState, Action, ActionError};

const SERVICE_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.service";
const SOCKET_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.socket";
const TMPFILES_SRC: &str = "/nix/var/nix/profiles/default//lib/tmpfiles.d/nix-daemon.conf";
const TMPFILES_DEST: &str = "/etc/tmpfiles.d/nix-daemon.conf";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixDaemonService {
    action_state: ActionState,
}

impl ConfigureNixDaemonService {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<Self, ConfigureNixDaemonServiceError> {
        if !Path::new("/run/systemd/system").exists() {
            return Err(ConfigureNixDaemonServiceError::InitNotSupported);
        }
        Ok(Self {
            action_state: ActionState::Planned,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ConfigureNixDaemonService {
    type Error = ConfigureNixDaemonServiceError;
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Configure Nix daemon related settings with systemd".to_string(),
            vec![
                "Run `systemd-tempfiles --create --prefix=/nix/var/nix`".to_string(),
                "Run `systemctl link {SERVICE_SRC}`".to_string(),
                "Run `systemctl link {SOCKET_SRC}`".to_string(),
                "Run `systemctl daemon-reload`".to_string(),
            ],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self { action_state } = self;
        tracing::info!("Configuring nix daemon service");

        tracing::trace!(src = TMPFILES_SRC, dest = TMPFILES_DEST, "Symlinking");
        tokio::fs::symlink(TMPFILES_SRC, TMPFILES_DEST)
            .await
            .map_err(|e| {
                Self::Error::Symlink(PathBuf::from(TMPFILES_SRC), PathBuf::from(TMPFILES_DEST), e)
            })?;

        execute_command(
            Command::new("systemd-tmpfiles")
                .arg("--create")
                .arg("--prefix=/nix/var/nix"),
        )
        .await.map_err(Self::Error::CommandFailed)?;

        execute_command(
            Command::new("systemctl").arg("link").arg(SERVICE_SRC),
        )
        .await.map_err(Self::Error::CommandFailed)?;

        execute_command(Command::new("systemctl").arg("link").arg(SOCKET_SRC)).await.map_err(Self::Error::CommandFailed)?;

        execute_command(Command::new("systemctl").arg("daemon-reload")).await.map_err(Self::Error::CommandFailed)?;

        *action_state = ActionState::Completed;
        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}



impl From<ConfigureNixDaemonService> for Action {
    fn from(v: ConfigureNixDaemonService) -> Self {
        Action::ConfigureNixDaemonService(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum ConfigureNixDaemonServiceError {
    #[error("Symlinking from `{0}` to `{1}`")]
    Symlink(
        std::path::PathBuf,
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error
    ),
    #[error("Command `{0}` failed to execute")]
    CommandFailed(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error
    ),
    #[error("No supported init system found")]
    InitNotSupported,
}
