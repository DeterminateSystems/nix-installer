use std::path::{Path, PathBuf};

use target_lexicon::OperatingSystem;
use tokio::fs::remove_file;
use tokio::process::Command;

use crate::action::StatefulAction;
use crate::execute_command;

use crate::{
    action::{Action, ActionDescription},
    BoxableError,
};

const SERVICE_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.service";
const SOCKET_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.socket";
const TMPFILES_SRC: &str = "/nix/var/nix/profiles/default/lib/tmpfiles.d/nix-daemon.conf";
const TMPFILES_DEST: &str = "/etc/tmpfiles.d/nix-daemon.conf";
const DARWIN_NIX_DAEMON_DEST: &str = "/Library/LaunchDaemons/org.nixos.nix-daemon.plist";

/**
Run systemd utilities to configure the Nix daemon
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixDaemonService {}

impl ConfigureNixDaemonService {
    #[tracing::instrument(skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, Box<dyn std::error::Error + Send + Sync>> {
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

        Ok(Self {}.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "configure_nix_daemon")]
impl Action for ConfigureNixDaemonService {
    fn tracing_synopsis(&self) -> String {
        "Configure Nix daemon related settings with systemd".to_string()
    }
    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![
                "Run `systemd-tempfiles --create --prefix=/nix/var/nix`".to_string(),
                "Run `systemctl link {SERVICE_SRC}`".to_string(),
                "Run `systemctl link {SOCKET_SRC}`".to_string(),
                "Run `systemctl daemon-reload`".to_string(),
            ],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let Self {} = self;

        match OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
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
                        .process_group(0)
                        .arg("load")
                        .arg(DARWIN_NIX_DAEMON_DEST)
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;
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
                        .process_group(0)
                        .arg("--create")
                        .arg("--prefix=/nix/var/nix")
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;

                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("link")
                        .arg(SERVICE_SRC)
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;

                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("link")
                        .arg(SOCKET_SRC)
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;

                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("daemon-reload")
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;

                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("enable")
                        .arg("--now")
                        .arg("nix-daemon.socket"),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;
            },
        };

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
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

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                execute_command(
                    Command::new("launchctl")
                        .process_group(0)
                        .arg("unload")
                        .arg(DARWIN_NIX_DAEMON_DEST),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;
            },
            _ => {
                // We separate stop and disable (instead of using `--now`) to avoid cases where the service isn't started, but is enabled.

                let socket_is_active = is_active("nix-daemon.socket").await?;
                let socket_is_enabled = is_enabled("nix-daemon.socket").await?;
                let service_is_active = is_active("nix-daemon.service").await?;
                let service_is_enabled = is_enabled("nix-daemon.service").await?;

                if socket_is_active {
                    execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["stop", "nix-daemon.socket"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;
                }

                if socket_is_enabled {
                    execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["disable", "nix-daemon.socket"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;
                }

                if service_is_active {
                    execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["stop", "nix-daemon.service"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;
                }

                if service_is_enabled {
                    execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["disable", "nix-daemon.service"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;
                }

                execute_command(
                    Command::new("systemd-tmpfiles")
                        .process_group(0)
                        .arg("--remove")
                        .arg("--prefix=/nix/var/nix")
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;

                remove_file(TMPFILES_DEST).await.map_err(|e| {
                    ConfigureNixDaemonServiceError::RemoveFile(PathBuf::from(TMPFILES_DEST), e)
                        .boxed()
                })?;

                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("daemon-reload")
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(|e| ConfigureNixDaemonServiceError::Command(e).boxed())?;
            },
        };

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigureNixDaemonServiceError {
    #[error("Symlinking from `{0}` to `{1}`")]
    Symlink(
        std::path::PathBuf,
        std::path::PathBuf,
        #[source] std::io::Error,
    ),
    #[error("Set mode `{0}` on `{1}`")]
    SetPermissions(u32, std::path::PathBuf, #[source] std::io::Error),
    #[error("Command failed to execute")]
    Command(#[source] std::io::Error),
    #[error("Remove file `{0}`")]
    RemoveFile(std::path::PathBuf, #[source] std::io::Error),
    #[error("Copying file `{0}` to `{1}`")]
    Copy(
        std::path::PathBuf,
        std::path::PathBuf,
        #[source] std::io::Error,
    ),
    #[error("No supported init system found")]
    InitNotSupported,
}

async fn is_active(unit: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let output = Command::new("systemctl")
        .arg("is-active")
        .arg(unit)
        .output()
        .await?;
    if String::from_utf8(output.stdout)?.starts_with("active") {
        tracing::trace!(%unit, "Is active");
        Ok(true)
    } else {
        tracing::trace!(%unit, "Is not active");
        Ok(false)
    }
}

async fn is_enabled(unit: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let output = Command::new("systemctl")
        .arg("is-enabled")
        .arg(unit)
        .output()
        .await?;
    let stdout = String::from_utf8(output.stdout)?;
    if stdout.starts_with("enabled") || stdout.starts_with("linked") {
        tracing::trace!(%unit, "Is enabled");
        Ok(true)
    } else {
        tracing::trace!(%unit, "Is not enabled");
        Ok(false)
    }
}
