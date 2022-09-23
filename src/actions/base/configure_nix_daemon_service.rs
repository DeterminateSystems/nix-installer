use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::process::Command;

use crate::{execute_command, HarmonicError};

use crate::actions::{ActionDescription, Actionable, ActionState, Action};

const SERVICE_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.service";
const SOCKET_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.socket";
const TMPFILES_SRC: &str = "/nix/var/nix/profiles/default//lib/tmpfiles.d/nix-daemon.conf";
const TMPFILES_DEST: &str = "/etc/tmpfiles.d/nix-daemon.conf";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixDaemonService {}

impl ConfigureNixDaemonService {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<Self, HarmonicError> {
        if !Path::new("/run/systemd/system").exists() {
            return Err(HarmonicError::InitNotSupported);
        }
        Ok(Self {})
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<ConfigureNixDaemonService> {
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
        tracing::info!("Configuring nix daemon service");

        tracing::trace!(src = TMPFILES_SRC, dest = TMPFILES_DEST, "Symlinking");
        tokio::fs::symlink(TMPFILES_SRC, TMPFILES_DEST)
            .await
            .map_err(|e| {
                HarmonicError::Symlink(PathBuf::from(TMPFILES_SRC), PathBuf::from(TMPFILES_DEST), e)
            })?;

        execute_command(
            Command::new("systemd-tmpfiles")
                .arg("--create")
                .arg("--prefix=/nix/var/nix"),
            false,
        )
        .await?;

        execute_command(
            Command::new("systemctl").arg("link").arg(SERVICE_SRC),
            false,
        )
        .await?;

        execute_command(Command::new("systemctl").arg("link").arg(SOCKET_SRC), false).await?;

        execute_command(Command::new("systemctl").arg("daemon-reload"), false).await?;

        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}



impl From<ActionState<ConfigureNixDaemonService>> for ActionState<Action> {
    fn from(v: ActionState<ConfigureNixDaemonService>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::ConfigureNixDaemonService(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::ConfigureNixDaemonService(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::ConfigureNixDaemonService(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum ConfigureNixDaemonServiceError {

}
