use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};
use crate::settings::InitSystem;

#[cfg(target_os = "linux")]
const SERVICE_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.service";
#[cfg(target_os = "linux")]
const SERVICE_DEST: &str = "/etc/systemd/system/nix-daemon.service";
#[cfg(target_os = "linux")]
const SOCKET_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.socket";
#[cfg(target_os = "linux")]
const SOCKET_DEST: &str = "/etc/systemd/system/nix-daemon.socket";
#[cfg(target_os = "linux")]
const TMPFILES_SRC: &str = "/nix/var/nix/profiles/default/lib/tmpfiles.d/nix-daemon.conf";
#[cfg(target_os = "linux")]
const TMPFILES_DEST: &str = "/etc/tmpfiles.d/nix-daemon.conf";
#[cfg(target_os = "macos")]
const DARWIN_NIX_DAEMON_DEST: &str = "/Library/LaunchDaemons/org.nixos.nix-daemon.plist";
#[cfg(target_os = "macos")]
const DARWIN_NIX_DAEMON_SOURCE: &str =
    "/nix/var/nix/profiles/default/Library/LaunchDaemons/org.nixos.nix-daemon.plist";
/**
Configure the init to run the Nix daemon
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureInitService {
    init: InitSystem,
    start_daemon: bool,
}

impl ConfigureInitService {
    #[cfg(target_os = "linux")]
    async fn check_if_systemd_unit_exists(src: &str, dest: &str) -> Result<(), ActionError> {
        // TODO: once we have a way to communicate interaction between the library and the cli,
        // interactively ask for permission to remove the file

        let unit_src = PathBuf::from(src);
        // NOTE: Check if the unit file already exists...
        let unit_dest = PathBuf::from(dest);
        if unit_dest.exists() {
            if unit_dest.is_symlink() {
                let link_dest = tokio::fs::read_link(&unit_dest)
                    .await
                    .map_err(|e| ActionError::ReadSymlink(unit_dest.clone(), e))?;
                if link_dest != unit_src {
                    return Err(ActionError::SymlinkExists(unit_dest));
                }
            } else {
                return Err(ActionError::FileExists(unit_dest));
            }
        }
        // NOTE: ...and if there are any overrides in the most well-known places for systemd
        if Path::new(&format!("{dest}.d")).exists() {
            return Err(ActionError::DirExists(PathBuf::from(format!("{dest}.d"))));
        }

        Ok(())
    }
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        init: InitSystem,
        start_daemon: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        match init {
            #[cfg(target_os = "macos")]
            InitSystem::Launchd => {
                // No plan checks, yet
            },
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => {
                Self::check_if_systemd_unit_exists(SERVICE_SRC, SERVICE_DEST).await?;
                Self::check_if_systemd_unit_exists(SOCKET_SRC, SOCKET_DEST).await?;
            },
            #[cfg(not(target_os = "macos"))]
            InitSystem::None => {
                // Nothing here, no init system
            },
        };

        Ok(Self { init, start_daemon }.into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "configure_init_service")]
impl Action for ConfigureInitService {
    fn action_tag() -> ActionTag {
        ActionTag("configure_init_service")
    }
    fn tracing_synopsis(&self) -> String {
        match self.init {
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => "Configure Nix daemon related settings with systemd".to_string(),
            #[cfg(target_os = "macos")]
            InitSystem::Launchd => {
                "Configure Nix daemon related settings with launchctl".to_string()
            },
            #[cfg(not(target_os = "macos"))]
            InitSystem::None => "Leave the Nix daemon unconfigured".to_string(),
        }
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "configure_init_service",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let mut vec = Vec::new();
        match self.init {
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => {
                let mut explanation = vec![
                    "Run `systemd-tempfiles --create --prefix=/nix/var/nix`".to_string(),
                    format!("Symlink `{SERVICE_SRC}` to `{SERVICE_DEST}`"),
                    format!("Symlink `{SOCKET_SRC}` to `{SOCKET_DEST}`"),
                    "Run `systemctl daemon-reload`".to_string(),
                ];
                if self.start_daemon {
                    explanation.push(format!("Run `systemctl enable --now {SOCKET_SRC}`"));
                }
                vec.push(ActionDescription::new(self.tracing_synopsis(), explanation))
            },
            #[cfg(target_os = "macos")]
            InitSystem::Launchd => {
                let mut explanation = vec![format!(
                    "Copy `{DARWIN_NIX_DAEMON_SOURCE}` to `DARWIN_NIX_DAEMON_DEST`"
                )];
                if self.start_daemon {
                    explanation.push(format!("Run `launchctl load {DARWIN_NIX_DAEMON_DEST}`"));
                }
                vec.push(ActionDescription::new(self.tracing_synopsis(), explanation))
            },
            #[cfg(not(target_os = "macos"))]
            InitSystem::None => (),
        }
        vec
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self { init, start_daemon } = self;

        match init {
            #[cfg(target_os = "macos")]
            InitSystem::Launchd => {
                let src = std::path::Path::new(DARWIN_NIX_DAEMON_SOURCE);
                tokio::fs::copy(src.clone(), DARWIN_NIX_DAEMON_DEST)
                    .await
                    .map_err(|e| {
                        ActionError::Copy(
                            src.to_path_buf(),
                            PathBuf::from(DARWIN_NIX_DAEMON_DEST),
                            e,
                        )
                    })?;

                execute_command(
                    Command::new("launchctl")
                        .process_group(0)
                        .args(&["load", "-w"])
                        .arg(DARWIN_NIX_DAEMON_DEST)
                        .stdin(std::process::Stdio::null()),
                )
                .await?;

                if *start_daemon {
                    execute_command(
                        Command::new("launchctl")
                            .process_group(0)
                            .arg("kickstart")
                            .arg("-k")
                            .arg("system/org.nixos.nix-daemon")
                            .stdin(std::process::Stdio::null()),
                    )
                    .await?;
                }
            },
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => {
                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("daemon-reload")
                        .stdin(std::process::Stdio::null()),
                )
                .await?;
                // The goal state is the `socket` enabled and active, the service not enabled and stopped (it activates via socket activation)
                let socket_was_active = if is_active("nix-daemon.socket").await? {
                    stop("nix-daemon.socket").await?;
                    true
                } else {
                    false
                };
                if is_enabled("nix-daemon.service").await? {
                    let now = is_active("nix-daemon.socket").await?;
                    disable("nix-daemon.service", now).await?;
                };

                tracing::trace!(src = TMPFILES_SRC, dest = TMPFILES_DEST, "Symlinking");
                if !Path::new(TMPFILES_DEST).exists() {
                    tokio::fs::symlink(TMPFILES_SRC, TMPFILES_DEST)
                        .await
                        .map_err(|e| {
                            ActionError::Symlink(
                                PathBuf::from(TMPFILES_SRC),
                                PathBuf::from(TMPFILES_DEST),
                                e,
                            )
                        })?;
                }

                execute_command(
                    Command::new("systemd-tmpfiles")
                        .process_group(0)
                        .arg("--create")
                        .arg("--prefix=/nix/var/nix")
                        .stdin(std::process::Stdio::null()),
                )
                .await?;

                // TODO: once we have a way to communicate interaction between the library and the
                // cli, interactively ask for permission to remove the file

                Self::check_if_systemd_unit_exists(SERVICE_SRC, SERVICE_DEST).await?;
                if !Path::new(SERVICE_DEST).exists() {
                    tokio::fs::symlink(SERVICE_SRC, SERVICE_DEST)
                        .await
                        .map_err(|e| {
                            ActionError::Symlink(
                                PathBuf::from(SERVICE_SRC),
                                PathBuf::from(SERVICE_DEST),
                                e,
                            )
                        })?;
                }

                Self::check_if_systemd_unit_exists(SOCKET_SRC, SOCKET_DEST).await?;
                if !Path::new(SOCKET_DEST).exists() {
                    tokio::fs::symlink(SOCKET_SRC, SOCKET_DEST)
                        .await
                        .map_err(|e| {
                            ActionError::Symlink(
                                PathBuf::from(SOCKET_SRC),
                                PathBuf::from(SOCKET_DEST),
                                e,
                            )
                        })?;
                }

                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("daemon-reload")
                        .stdin(std::process::Stdio::null()),
                )
                .await?;

                if *start_daemon || socket_was_active {
                    enable("nix-daemon.socket", true).await?;
                } else {
                    enable("nix-daemon.socket", false).await?;
                }
            },
            #[cfg(not(target_os = "macos"))]
            InitSystem::None => {
                // Nothing here, no init system
            },
        };

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        match self.init {
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => {
                vec![ActionDescription::new(
                    "Unconfigure Nix daemon related settings with systemd".to_string(),
                    vec![
                        "Run `systemctl disable {SOCKET_SRC}`".to_string(),
                        "Run `systemctl disable {SERVICE_SRC}`".to_string(),
                        "Run `systemd-tempfiles --remove --prefix=/nix/var/nix`".to_string(),
                        "Run `systemctl daemon-reload`".to_string(),
                    ],
                )]
            },
            #[cfg(target_os = "macos")]
            InitSystem::Launchd => {
                vec![ActionDescription::new(
                    "Unconfigure Nix daemon related settings with launchctl".to_string(),
                    vec![format!("Run `launchctl unload {DARWIN_NIX_DAEMON_DEST}`")],
                )]
            },
            #[cfg(not(target_os = "macos"))]
            InitSystem::None => Vec::new(),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        match self.init {
            #[cfg(target_os = "macos")]
            InitSystem::Launchd => {
                execute_command(
                    Command::new("launchctl")
                        .process_group(0)
                        .arg("unload")
                        .arg(DARWIN_NIX_DAEMON_DEST),
                )
                .await?;
            },
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => {
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
                    .await?;
                }

                if socket_is_enabled {
                    execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["disable", "nix-daemon.socket"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await?;
                }

                if service_is_active {
                    execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["stop", "nix-daemon.service"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await?;
                }

                if service_is_enabled {
                    execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["disable", "nix-daemon.service"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await?;
                }

                execute_command(
                    Command::new("systemd-tmpfiles")
                        .process_group(0)
                        .arg("--remove")
                        .arg("--prefix=/nix/var/nix")
                        .stdin(std::process::Stdio::null()),
                )
                .await?;

                tokio::fs::remove_file(TMPFILES_DEST)
                    .await
                    .map_err(|e| ActionError::Remove(PathBuf::from(TMPFILES_DEST), e))?;

                execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("daemon-reload")
                        .stdin(std::process::Stdio::null()),
                )
                .await?;
            },
            #[cfg(not(target_os = "macos"))]
            InitSystem::None => {
                // Nothing here, no init
            },
        };

        Ok(())
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ConfigureNixDaemonServiceError {
    #[error("No supported init system found")]
    InitNotSupported,
}

#[cfg(target_os = "linux")]
async fn stop(unit: &str) -> Result<(), ActionError> {
    let mut command = Command::new("systemctl");
    command.arg("stop");
    command.arg(unit);
    let output = command
        .output()
        .await
        .map_err(|e| ActionError::command(&command, e))?;
    match output.status.success() {
        true => Ok(()),
        false => Err(ActionError::command_output(&command, output)),
    }
}

#[cfg(target_os = "linux")]
async fn enable(unit: &str, now: bool) -> Result<(), ActionError> {
    let mut command = Command::new("systemctl");
    command.arg("enable");
    command.arg(unit);
    if now {
        command.arg("--now");
    }
    let output = command
        .output()
        .await
        .map_err(|e| ActionError::command(&command, e))?;
    match output.status.success() {
        true => Ok(()),
        false => Err(ActionError::command_output(&command, output)),
    }
}

#[cfg(target_os = "linux")]
async fn disable(unit: &str, now: bool) -> Result<(), ActionError> {
    let mut command = Command::new("systemctl");
    command.arg("disable");
    command.arg(unit);
    if now {
        command.arg("--now");
    }
    let output = command
        .output()
        .await
        .map_err(|e| ActionError::command(&command, e))?;
    match output.status.success() {
        true => Ok(()),
        false => Err(ActionError::command_output(&command, output)),
    }
}

#[cfg(target_os = "linux")]
async fn is_active(unit: &str) -> Result<bool, ActionError> {
    let mut command = Command::new("systemctl");
    command.arg("is-active");
    command.arg(unit);
    let output = command
        .output()
        .await
        .map_err(|e| ActionError::command(&command, e))?;
    if String::from_utf8(output.stdout)?.starts_with("active") {
        tracing::trace!(%unit, "Is active");
        Ok(true)
    } else {
        tracing::trace!(%unit, "Is not active");
        Ok(false)
    }
}

#[cfg(target_os = "linux")]
async fn is_enabled(unit: &str) -> Result<bool, ActionError> {
    let mut command = Command::new("systemctl");
    command.arg("is-enabled");
    command.arg(unit);
    let output = command
        .output()
        .await
        .map_err(|e| ActionError::command(&command, e))?;
    let stdout = String::from_utf8(output.stdout)?;
    if stdout.starts_with("enabled") || stdout.starts_with("linked") {
        tracing::trace!(%unit, "Is enabled");
        Ok(true)
    } else {
        tracing::trace!(%unit, "Is not enabled");
        Ok(false)
    }
}
