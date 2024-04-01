#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag, StatefulAction};
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
const DARWIN_NIX_ENTERPRISE_DEST: &str =
    "/Library/LaunchDaemons/systems.determinate.nix-daemon.plist";
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
    nix_enterprise: bool,
    start_daemon: bool,
}

impl ConfigureInitService {
    #[cfg(target_os = "linux")]
    async fn check_if_systemd_unit_exists(src: &str, dest: &str) -> Result<(), ActionErrorKind> {
        // TODO: once we have a way to communicate interaction between the library and the cli,
        // interactively ask for permission to remove the file

        let unit_src = PathBuf::from(src);
        // NOTE: Check if the unit file already exists...
        let unit_dest = PathBuf::from(dest);
        if unit_dest.exists() {
            if unit_dest.is_symlink() {
                let link_dest = tokio::fs::read_link(&unit_dest)
                    .await
                    .map_err(|e| ActionErrorKind::ReadSymlink(unit_dest.clone(), e))?;
                if link_dest != unit_src {
                    return Err(ActionErrorKind::SymlinkExists(unit_dest));
                }
            } else {
                return Err(ActionErrorKind::FileExists(unit_dest));
            }
        }
        // NOTE: ...and if there are any overrides in the most well-known places for systemd
        if Path::new(&format!("{dest}.d")).exists() {
            return Err(ActionErrorKind::DirExists(PathBuf::from(format!(
                "{dest}.d"
            ))));
        }

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        init: InitSystem,
        nix_enterprise: bool,
        start_daemon: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        match init {
            #[cfg(target_os = "macos")]
            InitSystem::Launchd => {
                // No plan checks, yet
            },
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => {
                // If `no_start_daemon` is set, then we don't require a running systemd,
                // so we don't need to check if `/run/systemd/system` exists.
                if start_daemon {
                    // If /run/systemd/system exists, we can be reasonably sure the machine is booted
                    // with systemd: https://www.freedesktop.org/software/systemd/man/sd_booted.html
                    if !Path::new("/run/systemd/system").exists() {
                        return Err(Self::error(ActionErrorKind::SystemdMissing));
                    }
                }

                if which::which("systemctl").is_err() {
                    return Err(Self::error(ActionErrorKind::SystemdMissing));
                }

                Self::check_if_systemd_unit_exists(SERVICE_SRC, SERVICE_DEST)
                    .await
                    .map_err(Self::error)?;
                Self::check_if_systemd_unit_exists(SOCKET_SRC, SOCKET_DEST)
                    .await
                    .map_err(Self::error)?;
            },
            #[cfg(target_os = "linux")]
            InitSystem::None => {
                // Nothing here, no init system
            },
        };

        Ok(Self {
            init,
            nix_enterprise,
            start_daemon,
        }
        .into())
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
                    "Run `systemd-tmpfiles --create --prefix=/nix/var/nix`".to_string(),
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
        let Self {
            init,
            nix_enterprise: _,
            start_daemon,
        } = self;

        match init {
            #[cfg(target_os = "macos")]
            InitSystem::Launchd => {
                let daemon_file: &str;
                let domain = "system";
                let service: &str;

                if self.nix_enterprise {
                    daemon_file = DARWIN_NIX_ENTERPRISE_DEST;
                    service = "systems.determinate.nix-daemon";

                    let generated_plist = generate_plist();

                    let mut options = tokio::fs::OpenOptions::new();
                    options.create(true).write(true).read(true);

                    let mut file = options.open(&daemon_file).await.map_err(|e| {
                        Self::error(ActionErrorKind::Open(PathBuf::from(daemon_file), e))
                    })?;

                    let mut buf = Vec::new();
                    plist::to_writer_xml(&mut buf, &generated_plist).map_err(Self::error)?;
                    file.write_all(&buf).await.map_err(|e| {
                        Self::error(ActionErrorKind::Write(PathBuf::from(daemon_file), e))
                    })?;
                } else {
                    let src = std::path::Path::new(DARWIN_NIX_DAEMON_SOURCE);
                    daemon_file = DARWIN_NIX_DAEMON_DEST;
                    service = "org.nixos.nix-daemon";
                    tokio::fs::copy(src, daemon_file).await.map_err(|e| {
                        Self::error(ActionErrorKind::Copy(
                            src.to_path_buf(),
                            PathBuf::from(daemon_file),
                            e,
                        ))
                    })?;
                }

                execute_command(
                    Command::new("launchctl")
                        .process_group(0)
                        .args(["load", "-w"])
                        .arg(daemon_file)
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(Self::error)?;

                let is_disabled = crate::action::macos::service_is_disabled(domain, service)
                    .await
                    .map_err(Self::error)?;
                if is_disabled {
                    execute_command(
                        Command::new("launchctl")
                            .process_group(0)
                            .arg("enable")
                            .arg(&format!("{domain}/{service}"))
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                }

                if *start_daemon {
                    execute_command(
                        Command::new("launchctl")
                            .process_group(0)
                            .arg("kickstart")
                            .arg("-k")
                            .arg(&format!("{domain}/{service}"))
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                }
            },
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => {
                if *start_daemon {
                    execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .arg("daemon-reload")
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                }
                // The goal state is the `socket` enabled and active, the service not enabled and stopped (it activates via socket activation)
                if is_enabled("nix-daemon.socket").await.map_err(Self::error)? {
                    disable("nix-daemon.socket", false)
                        .await
                        .map_err(Self::error)?;
                }
                let socket_was_active =
                    if is_active("nix-daemon.socket").await.map_err(Self::error)? {
                        stop("nix-daemon.socket").await.map_err(Self::error)?;
                        true
                    } else {
                        false
                    };
                if is_enabled("nix-daemon.service")
                    .await
                    .map_err(Self::error)?
                {
                    let now = is_active("nix-daemon.service").await.map_err(Self::error)?;
                    disable("nix-daemon.service", now)
                        .await
                        .map_err(Self::error)?;
                } else if is_active("nix-daemon.service").await.map_err(Self::error)? {
                    stop("nix-daemon.service").await.map_err(Self::error)?;
                };

                tracing::trace!(src = TMPFILES_SRC, dest = TMPFILES_DEST, "Symlinking");
                if !Path::new(TMPFILES_DEST).exists() {
                    tokio::fs::symlink(TMPFILES_SRC, TMPFILES_DEST)
                        .await
                        .map_err(|e| {
                            ActionErrorKind::Symlink(
                                PathBuf::from(TMPFILES_SRC),
                                PathBuf::from(TMPFILES_DEST),
                                e,
                            )
                        })
                        .map_err(Self::error)?;
                }

                execute_command(
                    Command::new("systemd-tmpfiles")
                        .process_group(0)
                        .arg("--create")
                        .arg("--prefix=/nix/var/nix")
                        .stdin(std::process::Stdio::null()),
                )
                .await
                .map_err(Self::error)?;

                // TODO: once we have a way to communicate interaction between the library and the
                // cli, interactively ask for permission to remove the file

                Self::check_if_systemd_unit_exists(SERVICE_SRC, SERVICE_DEST)
                    .await
                    .map_err(Self::error)?;
                if Path::new(SERVICE_DEST).exists() {
                    tracing::trace!(path = %SERVICE_DEST, "Removing");
                    tokio::fs::remove_file(SERVICE_DEST)
                        .await
                        .map_err(|e| ActionErrorKind::Remove(SERVICE_DEST.into(), e))
                        .map_err(Self::error)?;
                }
                tracing::trace!(src = %SERVICE_SRC, dest = %SERVICE_DEST, "Symlinking");
                tokio::fs::symlink(SERVICE_SRC, SERVICE_DEST)
                    .await
                    .map_err(|e| {
                        ActionErrorKind::Symlink(
                            PathBuf::from(SERVICE_SRC),
                            PathBuf::from(SERVICE_DEST),
                            e,
                        )
                    })
                    .map_err(Self::error)?;
                Self::check_if_systemd_unit_exists(SOCKET_SRC, SOCKET_DEST)
                    .await
                    .map_err(Self::error)?;
                if Path::new(SOCKET_DEST).exists() {
                    tracing::trace!(path = %SOCKET_DEST, "Removing");
                    tokio::fs::remove_file(SOCKET_DEST)
                        .await
                        .map_err(|e| ActionErrorKind::Remove(SOCKET_DEST.into(), e))
                        .map_err(Self::error)?;
                }

                tracing::trace!(src = %SOCKET_SRC, dest = %SOCKET_DEST, "Symlinking");
                tokio::fs::symlink(SOCKET_SRC, SOCKET_DEST)
                    .await
                    .map_err(|e| {
                        ActionErrorKind::Symlink(
                            PathBuf::from(SOCKET_SRC),
                            PathBuf::from(SOCKET_DEST),
                            e,
                        )
                    })
                    .map_err(Self::error)?;

                if *start_daemon {
                    execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .arg("daemon-reload")
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    .map_err(Self::error)?;
                }

                if *start_daemon || socket_was_active {
                    enable(SOCKET_SRC, true).await.map_err(Self::error)?;
                } else {
                    enable(SOCKET_SRC, false).await.map_err(Self::error)?;
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
                        format!("Run `systemctl disable {SOCKET_SRC}`"),
                        format!("Run `systemctl disable {SERVICE_SRC}`"),
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
        #[cfg_attr(target_os = "macos", allow(unused_mut))]
        let mut errors = vec![];

        match self.init {
            #[cfg(target_os = "macos")]
            InitSystem::Launchd => {
                execute_command(
                    Command::new("launchctl")
                        .process_group(0)
                        .arg("unload")
                        .arg(DARWIN_NIX_DAEMON_DEST),
                )
                .await
                .map_err(Self::error)?;
            },
            #[cfg(target_os = "linux")]
            InitSystem::Systemd => {
                // We separate stop and disable (instead of using `--now`) to avoid cases where the service isn't started, but is enabled.

                // These have to fail fast.
                let socket_is_active = is_active("nix-daemon.socket").await.map_err(Self::error)?;
                let socket_is_enabled =
                    is_enabled("nix-daemon.socket").await.map_err(Self::error)?;
                let service_is_active =
                    is_active("nix-daemon.service").await.map_err(Self::error)?;
                let service_is_enabled = is_enabled("nix-daemon.service")
                    .await
                    .map_err(Self::error)?;

                if socket_is_active {
                    if let Err(err) = execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["stop", "nix-daemon.socket"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    {
                        errors.push(err);
                    }
                }

                if socket_is_enabled {
                    if let Err(err) = execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["disable", "nix-daemon.socket"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    {
                        errors.push(err);
                    }
                }

                if service_is_active {
                    if let Err(err) = execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["stop", "nix-daemon.service"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    {
                        errors.push(err);
                    }
                }

                if service_is_enabled {
                    if let Err(err) = execute_command(
                        Command::new("systemctl")
                            .process_group(0)
                            .args(["disable", "nix-daemon.service"])
                            .stdin(std::process::Stdio::null()),
                    )
                    .await
                    {
                        errors.push(err);
                    }
                }

                if let Err(err) = execute_command(
                    Command::new("systemd-tmpfiles")
                        .process_group(0)
                        .arg("--remove")
                        .arg("--prefix=/nix/var/nix")
                        .stdin(std::process::Stdio::null()),
                )
                .await
                {
                    errors.push(err);
                }

                if let Err(err) = tokio::fs::remove_file(TMPFILES_DEST)
                    .await
                    .map_err(|e| ActionErrorKind::Remove(PathBuf::from(TMPFILES_DEST), e))
                {
                    errors.push(err);
                }

                if let Err(err) = execute_command(
                    Command::new("systemctl")
                        .process_group(0)
                        .arg("daemon-reload")
                        .stdin(std::process::Stdio::null()),
                )
                .await
                {
                    errors.push(err);
                }
            },
            #[cfg(not(target_os = "macos"))]
            InitSystem::None => {
                // Nothing here, no init
            },
        };

        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(Self::error(
                errors
                    .into_iter()
                    .next()
                    .expect("Expected 1 len Vec to have at least 1 item"),
            ))
        } else {
            Err(Self::error(ActionErrorKind::Multiple(errors)))
        }
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ConfigureNixDaemonServiceError {
    #[error("No supported init system found")]
    InitNotSupported,
}

#[cfg(target_os = "macos")]
#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DeterminateNixDaemonPlist {
    label: String,
    program: String,
    keep_alive: bool,
    run_at_load: bool,
    standard_error_path: String,
    standard_out_path: String,
    soft_resource_limits: ResourceLimits,
}

#[cfg(target_os = "macos")]
#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ResourceLimits {
    number_of_files: usize,
}

/// This function must be able to operate at both plan and execute time.
#[cfg(target_os = "macos")]
fn generate_plist() -> DeterminateNixDaemonPlist {
    DeterminateNixDaemonPlist {
        keep_alive: true,
        run_at_load: true,
        label: "systems.determinate.nix-daemon".into(),
        program: "/usr/local/bin/determinate-nix-for-macos".into(),
        standard_error_path: "/var/log/determinate-nix-daemon.log".into(),
        standard_out_path: "/var/log/determinate-nix-daemon.log".into(),
        soft_resource_limits: ResourceLimits {
            number_of_files: 1048576,
        },
    }
}

#[cfg(target_os = "linux")]
async fn stop(unit: &str) -> Result<(), ActionErrorKind> {
    let mut command = Command::new("systemctl");
    command.arg("stop");
    command.arg(unit);
    let output = command
        .output()
        .await
        .map_err(|e| ActionErrorKind::command(&command, e))?;
    match output.status.success() {
        true => {
            tracing::trace!(%unit, "Stopped");
            Ok(())
        },
        false => Err(ActionErrorKind::command_output(&command, output)),
    }
}

#[cfg(target_os = "linux")]
async fn enable(unit: &str, now: bool) -> Result<(), ActionErrorKind> {
    let mut command = Command::new("systemctl");
    command.arg("enable");
    command.arg(unit);
    if now {
        command.arg("--now");
    }
    let output = command
        .output()
        .await
        .map_err(|e| ActionErrorKind::command(&command, e))?;
    match output.status.success() {
        true => {
            tracing::trace!(%unit, %now, "Enabled unit");
            Ok(())
        },
        false => Err(ActionErrorKind::command_output(&command, output)),
    }
}

#[cfg(target_os = "linux")]
async fn disable(unit: &str, now: bool) -> Result<(), ActionErrorKind> {
    let mut command = Command::new("systemctl");
    command.arg("disable");
    command.arg(unit);
    if now {
        command.arg("--now");
    }
    let output = command
        .output()
        .await
        .map_err(|e| ActionErrorKind::command(&command, e))?;
    match output.status.success() {
        true => {
            tracing::trace!(%unit, %now, "Disabled unit");
            Ok(())
        },
        false => Err(ActionErrorKind::command_output(&command, output)),
    }
}

#[cfg(target_os = "linux")]
async fn is_active(unit: &str) -> Result<bool, ActionErrorKind> {
    let mut command = Command::new("systemctl");
    command.arg("is-active");
    command.arg(unit);
    let output = command
        .output()
        .await
        .map_err(|e| ActionErrorKind::command(&command, e))?;
    if String::from_utf8(output.stdout)?.starts_with("active") {
        tracing::trace!(%unit, "Is active");
        Ok(true)
    } else {
        tracing::trace!(%unit, "Is not active");
        Ok(false)
    }
}

#[cfg(target_os = "linux")]
async fn is_enabled(unit: &str) -> Result<bool, ActionErrorKind> {
    let mut command = Command::new("systemctl");
    command.arg("is-enabled");
    command.arg(unit);
    let output = command
        .output()
        .await
        .map_err(|e| ActionErrorKind::command(&command, e))?;
    let stdout = String::from_utf8(output.stdout)?;
    if stdout.starts_with("enabled") || stdout.starts_with("linked") {
        tracing::trace!(%unit, "Is enabled");
        Ok(true)
    } else {
        tracing::trace!(%unit, "Is not enabled");
        Ok(false)
    }
}
