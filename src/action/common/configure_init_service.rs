use std::path::Path;
use std::path::PathBuf;

use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag, StatefulAction};
use crate::execute_command;

use crate::action::{Action, ActionDescription};
use crate::settings::InitSystem;

const TMPFILES_SRC: &str = "/nix/var/nix/profiles/default/lib/tmpfiles.d/nix-daemon.conf";
const TMPFILES_DEST: &str = "/etc/tmpfiles.d/nix-daemon.conf";

const DARWIN_LAUNCHD_DOMAIN: &str = "system";

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SocketFile {
    pub name: String,
    pub src: UnitSrc,
    pub dest: PathBuf,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub enum UnitSrc {
    Path(PathBuf),
    Literal(String),
}

/**
Configure the init to run the Nix daemon
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "configure_init_service")]
pub struct ConfigureInitService {
    init: InitSystem,
    start_daemon: bool,
    // TODO(cole-h): make an enum so we can distinguish between "written out by another step" vs "actually there isn't one"
    service_src: Option<PathBuf>,
    service_name: Option<String>,
    service_dest: Option<PathBuf>,
    socket_files: Vec<SocketFile>,
}

impl ConfigureInitService {
    pub(crate) async fn check_if_systemd_unit_exists(
        src: &UnitSrc,
        dest: &Path,
    ) -> Result<(), ActionErrorKind> {
        // TODO: once we have a way to communicate interaction between the library and the cli,
        // interactively ask for permission to remove the file

        // NOTE: Check if the unit file already exists...
        let unit_dest = PathBuf::from(dest);
        if unit_dest.exists() {
            match src {
                UnitSrc::Path(unit_src) => {
                    if unit_dest.is_symlink() {
                        let link_dest = tokio::fs::read_link(&unit_dest)
                            .await
                            .map_err(|e| ActionErrorKind::ReadSymlink(unit_dest.clone(), e))?;
                        if link_dest != *unit_src {
                            return Err(ActionErrorKind::SymlinkExists(unit_dest));
                        }
                    } else {
                        return Err(ActionErrorKind::FileExists(unit_dest));
                    }
                },
                UnitSrc::Literal(content) => {
                    if unit_dest.is_symlink() {
                        return Err(ActionErrorKind::FileExists(unit_dest));
                    } else {
                        let actual_content = tokio::fs::read_to_string(&unit_dest)
                            .await
                            .map_err(|e| ActionErrorKind::Read(unit_dest.clone(), e))?;
                        if *content != actual_content {
                            return Err(ActionErrorKind::DifferentContent(unit_dest));
                        }
                    }
                },
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
        socket_files: Vec<SocketFile>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        match init {
            InitSystem::Launchd => {
                // No plan checks, yet
            },
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

                if let (Some(service_src), Some(service_dest)) =
                    (service_src.as_ref(), service_dest.as_ref())
                {
                    Self::check_if_systemd_unit_exists(
                        &UnitSrc::Path(service_src.to_path_buf()),
                        service_dest,
                    )
                    .await
                    .map_err(Self::error)?;
                }

                for SocketFile { src, dest, .. } in socket_files.iter() {
                    Self::check_if_systemd_unit_exists(src, dest)
                        .await
                        .map_err(Self::error)?;
                }
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
            socket_files,
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
                ];

                for SocketFile { src, dest, .. } in self.socket_files.iter() {
                    match src {
                        UnitSrc::Path(src) => {
                            explanation.push(format!(
                                "Symlink `{}` to `{}`",
                                src.display(),
                                dest.display()
                            ));
                        },
                        UnitSrc::Literal(_) => {
                            explanation.push(format!("Create `{}`", dest.display()));
                        },
                    }
                }
                explanation.push("Run `systemctl daemon-reload`".to_string());

                if self.start_daemon {
                    for SocketFile { name, .. } in self.socket_files.iter() {
                        explanation.push(format!("Run `systemctl enable --now {}`", name));
                    }
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
            socket_files,
        } = self;

        match init {
            InitSystem::Launchd => {
                let service_dest = service_dest
                    .as_ref()
                    .expect("service_dest should be set for Launchd");
                let service = service_name
                    .as_ref()
                    .expect("service_name should be set for Launchd");
                let domain = DARWIN_LAUNCHD_DOMAIN;

                if let Some(service_src) = service_src {
                    tokio::fs::copy(&service_src, service_dest)
                        .await
                        .map_err(|e| {
                            Self::error(ActionErrorKind::Copy(
                                service_src.clone(),
                                PathBuf::from(service_dest),
                                e,
                            ))
                        })?;
                }

                crate::action::macos::retry_bootstrap(&domain, &service_dest)
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
                let mut any_socket_was_active = false;
                for SocketFile { name, .. } in socket_files.iter() {
                    let is_active = is_active(name).await.map_err(Self::error)?;

                    if is_enabled(name).await.map_err(Self::error)? {
                        disable(name, is_active).await.map_err(Self::error)?;
                    } else if is_active {
                        stop(name).await.map_err(Self::error)?;
                    };

                    if is_active {
                        any_socket_was_active = true;
                    }
                }

                {
                    let is_active = is_active("nix-daemon.service").await.map_err(Self::error)?;

                    if is_enabled("nix-daemon.service")
                        .await
                        .map_err(Self::error)?
                    {
                        disable("nix-daemon.service", is_active)
                            .await
                            .map_err(Self::error)?;
                    } else if is_active {
                        stop("nix-daemon.service").await.map_err(Self::error)?;
                    };
                }

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

                if let Some(service_src) = service_src.as_ref() {
                    Self::check_if_systemd_unit_exists(
                        &UnitSrc::Path(service_src.to_path_buf()),
                        service_dest,
                    )
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
                    tokio::fs::symlink(service_src, service_dest)
                        .await
                        .map_err(|e| {
                            ActionErrorKind::Symlink(
                                service_src.clone(),
                                PathBuf::from(service_dest),
                                e,
                            )
                        })
                        .map_err(Self::error)?;
                }

                for SocketFile { src, dest, .. } in socket_files.iter() {
                    Self::check_if_systemd_unit_exists(src, dest)
                        .await
                        .map_err(Self::error)?;
                    if Path::new(dest).exists() {
                        tracing::trace!(path = %dest.display(), "Removing");
                        tokio::fs::remove_file(dest)
                            .await
                            .map_err(|e| ActionErrorKind::Remove(dest.into(), e))
                            .map_err(Self::error)?;
                    }

                    match src {
                        UnitSrc::Path(src) => {
                            tracing::trace!(src = %src.display(), dest = %dest.display(), "Symlinking");
                            tokio::fs::symlink(src, dest)
                                .await
                                .map_err(|e| {
                                    ActionErrorKind::Symlink(
                                        PathBuf::from(src),
                                        PathBuf::from(dest),
                                        e,
                                    )
                                })
                                .map_err(Self::error)?;
                        },
                        UnitSrc::Literal(content) => {
                            tracing::trace!(src = %content, dest = %dest.display(), "Writing");

                            tokio::fs::write(&dest, content)
                                .await
                                .map_err(|e| ActionErrorKind::Write(dest.clone(), e))
                                .map_err(Self::error)?;
                        },
                    }
                }

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

                for SocketFile { name, src, .. } in socket_files.iter() {
                    let enable_now = *start_daemon || any_socket_was_active;

                    match src {
                        UnitSrc::Path(path) => {
                            // NOTE(cole-h): we have to enable by path here because older systemd's
                            // (e.g. on our Ubuntu 16.04 test VMs) had faulty (or too- strict)
                            // symlink detection, which causes the symlink chain of
                            // `/etc/systemd/system/nix-daemon.socket` ->
                            // `/nix/var/nix/profiles/default` -> `/nix/store/............/nix-
                            // daemon.socket` to fail with "Failed to execute operation: Too many
                            // levels of symbolic links"
                            enable(path.display().to_string().as_ref(), enable_now)
                                .await
                                .map_err(Self::error)?;
                        },
                        UnitSrc::Literal(_) => {
                            enable(name, enable_now).await.map_err(Self::error)?;
                        },
                    }
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
                let mut steps = vec![];

                for SocketFile { name, .. } in self.socket_files.iter() {
                    steps.push(format!("Run `systemctl disable {}`", name));
                }

                steps.push(format!(
                    "Run `systemctl disable {0}`",
                    self.service_src
                        .as_ref()
                        .expect("service_src should be defined for systemd")
                        .display()
                ));
                steps.push("Run `systemd-tempfiles --remove --prefix=/nix/var/nix`".to_string());
                steps.push("Run `systemctl daemon-reload`".to_string());

                vec![ActionDescription::new(
                    "Unconfigure Nix daemon related settings with systemd".to_string(),
                    steps,
                )]
            },
            InitSystem::Launchd => {
                vec![ActionDescription::new(
                    "Unconfigure Nix daemon related settings with launchctl".to_string(),
                    vec![format!(
                        "Run `launchctl bootout {DARWIN_LAUNCHD_DOMAIN}/{0}`",
                        self.service_name
                            .as_ref()
                            .expect("service_name should be defined for launchd"),
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

                // check if the daemon is down up to 99 times, with 100ms of delay between each attempt
                for attempt in 1..100 {
                    tracing::trace!(attempt, "Checking to see if the daemon is down yet");
                    if execute_command(
                        Command::new("launchctl").process_group(0).arg("print").arg(
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
                    .is_err()
                    {
                        tracing::trace!(attempt, "Daemon is down");
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            },
            InitSystem::Systemd => {
                // We separate stop and disable (instead of using `--now`) to avoid cases where the service isn't started, but is enabled.

                // These have to fail fast.
                for SocketFile { name, .. } in self.socket_files.iter() {
                    let socket_is_active = is_active(name).await.map_err(Self::error)?;
                    let socket_is_enabled = is_enabled(name).await.map_err(Self::error)?;

                    if socket_is_active {
                        if let Err(err) = execute_command(
                            Command::new("systemctl")
                                .process_group(0)
                                .args(["stop", name])
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
                                .args(["disable", name])
                                .stdin(std::process::Stdio::null()),
                        )
                        .await
                        {
                            errors.push(err);
                        }
                    }
                }
                let service_is_active =
                    is_active("nix-daemon.service").await.map_err(Self::error)?;
                let service_is_enabled = is_enabled("nix-daemon.service")
                    .await
                    .map_err(Self::error)?;

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

                for socket in self.socket_files.iter() {
                    if let UnitSrc::Literal(_) = socket.src {
                        tracing::trace!(path = %socket.dest.display(), "Removing");
                        tokio::fs::remove_file(&socket.dest)
                            .await
                            .map_err(|e| ActionErrorKind::Remove(socket.dest.to_path_buf(), e))
                            .map_err(Self::error)?;
                    }
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
            tracing::trace!(unit = %unit, %now, "Enabled unit");
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
