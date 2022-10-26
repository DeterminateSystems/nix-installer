use std::path::{Path, PathBuf};

use serde::Serialize;
use target_lexicon::OperatingSystem;
use tokio::fs::remove_file;
use tokio::process::Command;

use crate::execute_command;

use crate::actions::{ActionDescription, ActionError, ActionState, Actionable};

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
    pub async fn plan() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        match OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => (),
            _ => {
                if !Path::new("/run/systemd/system").exists() {
                    return Err(ConfigureNixDaemonServiceError::InitNotSupported.boxed());
                }
            },
        };

        Ok(Self {
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "configure-nix-daemon")]
impl Actionable for ConfigureNixDaemonService {
    fn describe_execute(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
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
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { action_state } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Configuring nix daemon service");
            return Ok(());
        }
        tracing::debug!("Configuring nix daemon service");

        match OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                const DARWIN_NIX_DAEMON_DEST: &str =
                    "/Library/LaunchDaemons/org.nixos.nix-daemon.plist";

                let src = Path::new("/nix/var/nix/profiles/default/Library/LaunchDaemons/org.nixos.nix-daemon.plist");
                tokio::fs::copy(src.clone(), DARWIN_NIX_DAEMON_DEST)
                    .await
                    .map_err(|e| {
                        ConfigureNixDaemonServiceError::Copy(
                            src.to_path_buf(),
                            PathBuf::from(DARWIN_NIX_DAEMON_DEST),
                            e,
                        )
                        .boxed()
                    })?;

                execute_command(
                    Command::new("launchctl")
                        .arg("load")
                        .arg(DARWIN_NIX_DAEMON_DEST),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::CommandFailed(e).boxed())?;
            },
            _ => {
                tracing::trace!(src = TMPFILES_SRC, dest = TMPFILES_DEST, "Symlinking");
                tokio::fs::symlink(TMPFILES_SRC, TMPFILES_DEST)
                    .await
                    .map_err(|e| {
                        ConfigureNixDaemonServiceError::Symlink(
                            PathBuf::from(TMPFILES_SRC),
                            PathBuf::from(TMPFILES_DEST),
                            e,
                        )
                        .boxed()
                    })?;

                execute_command(
                    Command::new("systemd-tmpfiles")
                        .arg("--create")
                        .arg("--prefix=/nix/var/nix"),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::CommandFailed(e).boxed())?;

                execute_command(Command::new("systemctl").arg("link").arg(SERVICE_SRC))
                    .await
                    .map_err(|e| ConfigureNixDaemonServiceError::CommandFailed(e).boxed())?;

                execute_command(Command::new("systemctl").arg("link").arg(SOCKET_SRC))
                    .await
                    .map_err(|e| ConfigureNixDaemonServiceError::CommandFailed(e).boxed())?;

                execute_command(Command::new("systemctl").arg("daemon-reload"))
                    .await
                    .map_err(|e| ConfigureNixDaemonServiceError::CommandFailed(e).boxed())?;
            },
        };

        tracing::trace!("Configured nix daemon service");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            vec![ActionDescription::new(
                "Unconfigure Nix daemon related settings with systemd".to_string(),
                vec![
                    "Run `systemctl disable {SOCKET_SRC}`".to_string(),
                    "Run `systemctl disable {SERVICE_SRC}`".to_string(),
                    "Run `systemd-tempfiles --remove --prefix=/nix/var/nix`".to_string(),
                    "Run `systemctl daemon-reload`".to_string(),
                ],
            )]
        }
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self { action_state } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Unconfiguring nix daemon service");
            return Ok(());
        }
        tracing::debug!("Unconfiguring nix daemon service");

        // We don't need to do this! Systemd does it for us! (In fact, it's an error if we try to do this...)
        execute_command(Command::new("systemctl").args(["disable", SOCKET_SRC]))
            .await
            .map_err(|e| ConfigureNixDaemonServiceError::CommandFailed(e).boxed())?;

        execute_command(Command::new("systemctl").args(["disable", SERVICE_SRC]))
            .await
            .map_err(|e| ConfigureNixDaemonServiceError::CommandFailed(e).boxed())?;

        execute_command(
            Command::new("systemd-tmpfiles")
                .arg("--remove")
                .arg("--prefix=/nix/var/nix"),
        )
        .await
        .map_err(|e| ConfigureNixDaemonServiceError::CommandFailed(e).boxed())?;

        remove_file(TMPFILES_DEST).await.map_err(|e| {
            ConfigureNixDaemonServiceError::RemoveFile(PathBuf::from(TMPFILES_DEST), e).boxed()
        })?;

        execute_command(Command::new("systemctl").arg("daemon-reload"))
            .await
            .map_err(|e| ConfigureNixDaemonServiceError::CommandFailed(e).boxed())?;

        tracing::trace!("Unconfigured nix daemon service");
        *action_state = ActionState::Uncompleted;
        Ok(())
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
        std::io::Error,
    ),
    #[error("Command failed to execute")]
    CommandFailed(
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Remove file `{0}`")]
    RemoveFile(
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("Copying file `{0}` to `{1}`")]
    Copy(
        std::path::PathBuf,
        std::path::PathBuf,
        #[source]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        std::io::Error,
    ),
    #[error("No supported init system found")]
    InitNotSupported,
}
