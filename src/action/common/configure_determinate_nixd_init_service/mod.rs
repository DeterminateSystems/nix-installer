use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{span, Span};

use crate::action::common::configure_init_service::{SocketFile, UnitSrc};
use crate::action::{common::ConfigureInitService, Action, ActionDescription};
use crate::action::{ActionError, ActionErrorKind, ActionTag, StatefulAction};
use crate::settings::InitSystem;
use crate::util::OnMissing;

// Linux
const LINUX_NIXD_DAEMON_DEST: &str = "/etc/systemd/system/nix-daemon.service";
const LINUX_NIXD_SERVICE_NAME: &str = "nix-daemon.service";

// Darwin
pub(crate) const DARWIN_NIXD_DAEMON_DEST: &str =
    "/Library/LaunchDaemons/systems.determinate.nix-daemon.plist";
const DARWIN_NIXD_SERVICE_NAME: &str = "systems.determinate.nix-daemon";

/**
Configure the init to run the Nix daemon
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(
    tag = "action_name",
    rename = "configure_determinate_nixd_init_service"
)]
pub struct ConfigureDeterminateNixdInitService {
    init: InitSystem,
    configure_init_service: StatefulAction<ConfigureInitService>,
}

impl ConfigureDeterminateNixdInitService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        init: InitSystem,
        start_daemon: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let service_dest: Option<PathBuf> = match init {
            InitSystem::Launchd => {
                // NOTE(cole-h): if the upstream daemon exists and we're installing determinate-
                // nixd, we need to remove the old daemon unit -- we used to have a bug[1] where
                // these service files wouldn't get removed, so we can't rely on them not being
                // there after phase 1 of the uninstall
                // [1]: https://github.com/DeterminateSystems/nix-installer/pull/1266
                crate::util::remove_file(
                    Path::new(super::configure_upstream_init_service::DARWIN_NIX_DAEMON_DEST),
                    OnMissing::Ignore,
                )
                .await
                .map_err(|e| {
                    Self::error(ActionErrorKind::Remove(
                        super::configure_upstream_init_service::DARWIN_NIX_DAEMON_DEST.into(),
                        e,
                    ))
                })?;

                Some(DARWIN_NIXD_DAEMON_DEST.into())
            },
            InitSystem::Systemd => Some(LINUX_NIXD_DAEMON_DEST.into()),
            InitSystem::None => None,
        };
        let service_name: Option<String> = match init {
            InitSystem::Launchd => Some(DARWIN_NIXD_SERVICE_NAME.into()),
            InitSystem::Systemd => Some(LINUX_NIXD_SERVICE_NAME.into()),
            _ => None,
        };

        let service_file: Option<UnitSrc> = match init {
            InitSystem::Launchd => {
                let generated_plist = generate_plist();

                let mut buf = Vec::new();
                plist::to_writer_xml(&mut buf, &generated_plist).map_err(Self::error)?;

                Some(UnitSrc::Literal(
                    String::from_utf8(buf)
                        .map_err(|e| Self::error(ActionErrorKind::FromUtf8(e)))?,
                ))
            },
            InitSystem::Systemd => Some(UnitSrc::Literal(
                include_str!("./nix-daemon.determinate-nixd.service").to_string(),
            )),
            InitSystem::None => None {},
        };

        let configure_init_service = ConfigureInitService::plan(
            init,
            start_daemon,
            service_file,
            service_dest,
            service_name,
            vec![
                SocketFile {
                    name: "nix-daemon.socket".into(),
                    src: UnitSrc::Literal(
                        include_str!("./nix-daemon.determinate-nixd.socket").to_string(),
                    ),
                    dest: "/etc/systemd/system/nix-daemon.socket".into(),
                },
                SocketFile {
                    name: "determinate-nixd.socket".into(),
                    src: UnitSrc::Literal(
                        include_str!("./nixd.determinate-nixd.socket").to_string(),
                    ),
                    dest: "/etc/systemd/system/determinate-nixd.socket".into(),
                },
            ],
        )
        .await
        .map_err(Self::error)?;

        Ok(Self {
            init,
            configure_init_service,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "configure_determinate_nixd_init_service")]
impl Action for ConfigureDeterminateNixdInitService {
    fn action_tag() -> ActionTag {
        ActionTag("configure_determinate_nixd_init_service")
    }
    fn tracing_synopsis(&self) -> String {
        "Configure the Determinate Nix daemon".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "configure_determinate_nixd_init_service"
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![self.configure_init_service.tracing_synopsis()],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        self.configure_init_service
            .try_execute()
            .await
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Remove the Determinate Nix daemon".to_string(),
            vec![self.configure_init_service.tracing_synopsis()],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        self.configure_init_service.try_revert().await?;

        Ok(())
    }
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DeterminateNixDaemonPlist {
    label: String,
    program_arguments: Vec<String>,
    run_at_load: bool,
    sockets: HashMap<String, Socket>,
    standard_error_path: String,
    standard_out_path: String,
    soft_resource_limits: ResourceLimits,
    hard_resource_limits: ResourceLimits,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ResourceLimits {
    number_of_files: usize,
    number_of_processes: usize,
    stack: usize,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Socket {
    sock_family: SocketFamily,
    sock_passive: bool,
    sock_path_name: String,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
enum SocketFamily {
    Unix,
}

fn generate_plist() -> DeterminateNixDaemonPlist {
    DeterminateNixDaemonPlist {
        run_at_load: false,
        label: "systems.determinate.nix-daemon".into(),
        program_arguments: vec!["/usr/local/bin/determinate-nixd".into(), "daemon".into()],
        standard_error_path: "/var/log/determinate-nix-daemon.log".into(),
        standard_out_path: "/var/log/determinate-nix-daemon.log".into(),
        soft_resource_limits: ResourceLimits {
            number_of_files: 1024 * 1024,
            number_of_processes: 1024 * 1024,
            stack: 64 * 1024 * 1024,
        },
        hard_resource_limits: ResourceLimits {
            number_of_files: 1024 * 1024,
            number_of_processes: 1024 * 1024,
            stack: 64 * 1024 * 1024,
        },
        sockets: HashMap::from([
            (
                "determinate-nixd.socket".to_string(),
                Socket {
                    sock_family: SocketFamily::Unix,
                    sock_passive: true,
                    sock_path_name: "/var/run/determinate-nixd.socket".into(),
                },
            ),
            (
                "nix-daemon.socket".to_string(),
                Socket {
                    sock_family: SocketFamily::Unix,
                    sock_passive: true,
                    sock_path_name: "/var/run/nix-daemon.socket".into(),
                },
            ),
        ]),
    }
}
