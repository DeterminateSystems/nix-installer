use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};
use crate::settings::InitSystem;

const SOCKET_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.socket";
const SOCKET_DEST: &str = "/etc/systemd/system/nix-daemon.socket";
const TMPFILES_SRC: &str = "/nix/var/nix/profiles/default/lib/tmpfiles.d/nix-daemon.conf";
const TMPFILES_DEST: &str = "/etc/tmpfiles.d/nix-daemon.conf";

const DARWIN_LAUNCHD_DOMAIN: &str = "system";

/**
Configure the init to run the Nix daemon
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureInitService {
    init: InitSystem,
    start_daemon: bool,
    // TODO(cole-h): make an enum so we can distinguish between "written out by another step" vs "actually there isn't one"
    service_src: Option<PathBuf>,
    service_name: Option<String>,
    service_dest: Option<PathBuf>,
}

impl ConfigureInitService {
    pub(crate) async fn check_if_systemd_unit_exists(
        src: &Path,
        dest: &Path,
    ) -> Result<(), ActionErrorKind> {
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
        let dest_d = format!("{dest}.d", dest = dest.display());
        if Path::new(&dest_d).exists() {
            return Err(ActionErrorKind::DirExists(PathBuf::from(dest_d)));
        }

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        init: InitSystem,
        start_daemon: bool,
        service_src: Option<PathBuf>,
        service_dest: Option<PathBuf>,
        service_name: Option<String>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        match init {
            InitSystem::Launchd => {
                // No plan checks, yet
            },
            InitSystem::Systemd => {
                let service_src = service_src
                    .as_ref()
                    .expect("service_src should be defined for systemd");
                let service_dest = service_dest
                    .as_ref()
                    .expect("service_dest should be defined for systemd");

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

                Self::check_if_systemd_unit_exists(service_src, service_dest)
                    .await
                    .map_err(Self::error)?;
                Self::check_if_systemd_unit_exists(Path::new(SOCKET_SRC), Path::new(SOCKET_DEST))
                    .await
                    .map_err(Self::error)?;
            },
            InitSystem::None => {
                // Nothing here, no init system
            },
        };

        Ok(Self {
            init,
            start_daemon,
            service_src,
            service_dest,
            service_name,
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
            InitSystem::Systemd => "Configure Nix daemon related settings with systemd".to_string(),
            InitSystem::Launchd => {
                "Configure Nix daemon related settings with launchctl".to_string()
            },
            InitSystem::None => "Leave the Nix daemon unconfigured".to_string(),
        }
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "configure_init_service")
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let mut vec = Vec::new();
        match self.init {
            InitSystem::Systemd => {
                let mut explanation = vec![
                    "Run `systemd-tmpfiles --create --prefix=/nix/var/nix`".to_string(),
                    format!(
                        "Symlink `{0}` to `{1}`",
                        self.service_src
                            .as_ref()
                            .expect("service_src should be defined for systemd")
                            .display(),
                        self.service_dest
                            .as_ref()
                            .expect("service_src should be defined for systemd")
                            .display()
                    ),
                    format!("Symlink `{SOCKET_SRC}` to `{SOCKET_DEST}`"),
                    "Run `systemctl daemon-reload`".to_string(),
                ];
                if self.start_daemon {
                    explanation.push(format!("Run `systemctl enable --now {SOCKET_SRC}`"));
                }
                vec.push(ActionDescription::new(self.tracing_synopsis(), explanation))
            },
            InitSystem::Launchd => {
                let mut explanation = vec![];
                if let Some(service_src) = self.service_src.as_ref() {
                    explanation.push(format!(
                        "Copy `{0}` to `{1}`",
                        service_src.display(),
                        self.service_dest
                            .as_ref()
                            .expect("service_dest should be defined for launchd")
                            .display(),
                    ));
                }

                if self.start_daemon {
                    explanation.push(format!(
                        "Run `launchctl bootstrap {0}`",
                        self.service_dest
                            .as_ref()
                            .expect("service_dest should be defined for launchd")
                            .display(),
                    ));
                }
                vec.push(ActionDescription::new(self.tracing_synopsis(), explanation))
            },
            InitSystem::None => (),
        }
        vec
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            init,
            start_daemon,
            service_src,
            service_dest,
            service_name,
        } = self;

        match init {
            InitSystem::Launchd => {
                let service_src = service_src
                    .as_ref()
                    .expect("service_src should be defined for launchd");

                let service_dest = service_dest
                    .as_ref()
                    .expect("service_dest should be set for Launchd");
                let service = service_name
                    .as_ref()
                    .expect("service_name should be set for Launchd");
                let domain = DARWIN_LAUNCHD_DOMAIN;

                tokio::fs::copy(&service_src, service_dest)
                    .await
                    .map_err(|e| {
                        Self::error(ActionErrorKind::Copy(
                            service_src.clone(),
                            PathBuf::from(service_dest),
                            e,
                        ))
                    })?;

                execute_command(
                    Command::new("launchctl")
                        .process_group(0)
                        .arg("bootstrap")
                        .arg(domain)
                        .arg(service_dest)
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
            InitSystem::Systemd => {
                let service_src = service_src
                    .as_ref()
                    .expect("service_src should be defined for systemd");
                let service_dest = service_dest
                    .as_ref()
                    .expect("service_dest should be defined for systemd");

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

                Self::check_if_systemd_unit_exists(service_src, service_dest)
                    .await
                    .map_err(Self::error)?;
                if Path::new(service_dest).exists() {
                    tracing::trace!(path = %service_dest.display(), "Removing");
                    tokio::fs::remove_file(service_dest)
                        .await
                        .map_err(|e| ActionErrorKind::Remove(service_dest.into(), e))
                        .map_err(Self::error)?;
                }
                tracing::trace!(src = %service_src.display(), dest = %service_dest.display(), "Symlinking");
                tokio::fs::symlink(
                    &self
                        .service_src
                        .as_ref()
                        .expect("service_src should be defined for systemd"),
                    service_dest,
                )
                .await
                .map_err(|e| {
                    ActionErrorKind::Symlink(
                        self.service_src
                            .as_ref()
                            .expect("service_src should be defined for systemd")
                            .clone(),
                        PathBuf::from(service_dest),
                        e,
                    )
                })
                .map_err(Self::error)?;
                Self::check_if_systemd_unit_exists(Path::new(SOCKET_SRC), Path::new(SOCKET_DEST))
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
            InitSystem::None => {
                // Nothing here, no init system
            },
        };

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        match self.init {
            InitSystem::Systemd => {
                vec![ActionDescription::new(
                    "Unconfigure Nix daemon related settings with systemd".to_string(),
                    vec![
                        format!("Run `systemctl disable {SOCKET_SRC}`"),
                        format!(
                            "Run `systemctl disable {0}`",
                            self.service_src
                                .as_ref()
                                .expect("service_src should be defined for systemd")
                                .display()
                        ),
                        "Run `systemd-tempfiles --remove --prefix=/nix/var/nix`".to_string(),
                        "Run `systemctl daemon-reload`".to_string(),
                    ],
                )]
            },
            InitSystem::Launchd => {
                vec![ActionDescription::new(
                    "Unconfigure Nix daemon related settings with launchctl".to_string(),
                    vec![format!(
                        "Run `launchctl bootout {0}`",
                        self.service_dest
                            .as_ref()
                            .expect("service_dest should be defined for launchd")
                            .display(),
                    )],
                )]
            },
            InitSystem::None => Vec::new(),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];

        match self.init {
            InitSystem::Launchd => {
                execute_command(
                    Command::new("launchctl")
                        .process_group(0)
                        .arg("bootout")
                        .arg(
                            [
                                DARWIN_LAUNCHD_DOMAIN,
                                self.service_name
                                    .as_ref()
                                    .expect("service_name should be defined for launchd"),
                            ]
                            .join("/"),
                        ),
                )
                .await
                .map_err(Self::error)?;
            },
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

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ResourceLimits {
    number_of_files: usize,
}

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
