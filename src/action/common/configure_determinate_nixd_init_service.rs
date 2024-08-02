use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag, StatefulAction};

use crate::action::{common::ConfigureInitService, Action, ActionDescription};
use crate::settings::InitSystem;

// Linux
const SERVICE_DEST: &str = "/etc/systemd/system/nix-daemon.service";
pub const DETERMINATE_NIXD_SERVICE_SRC: &str = "/nix/determinate/nix-daemon.service";

// Darwin
const DARWIN_NIXD_DAEMON_DEST: &str = "/Library/LaunchDaemons/systems.determinate.nix-daemon.plist";
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
        let service_src: Option<PathBuf> = match init {
            InitSystem::Launchd => {
                // We'll write it out down in the execute step
                None
            },
            InitSystem::Systemd => Some(DETERMINATE_NIXD_SERVICE_SRC.into()),
            InitSystem::None => None,
        };
        let service_dest: Option<PathBuf> = match init {
            InitSystem::Launchd => Some(DARWIN_NIXD_DAEMON_DEST.into()),
            InitSystem::Systemd => Some(SERVICE_DEST.into()),
            InitSystem::None => None,
        };
        let service_name: Option<String> = match init {
            InitSystem::Launchd => Some(DARWIN_NIXD_SERVICE_NAME.into()),
            _ => None,
        };

        let configure_init_service =
            ConfigureInitService::plan(init, start_daemon, service_src, service_dest, service_name)
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
        let Self {
            init,
            configure_init_service,
        } = self;

        let daemon_file = DARWIN_NIXD_DAEMON_DEST;

        if *init == InitSystem::Launchd {
            // This is the only part that is actually different from configure_init_service, beyond variable parameters.

            let generated_plist = generate_plist();

            let mut options = tokio::fs::OpenOptions::new();
            options.create(true).write(true).read(true);

            let mut file = options
                .open(&daemon_file)
                .await
                .map_err(|e| Self::error(ActionErrorKind::Open(PathBuf::from(daemon_file), e)))?;

            let mut buf = Vec::new();
            plist::to_writer_xml(&mut buf, &generated_plist).map_err(Self::error)?;
            file.write_all(&buf)
                .await
                .map_err(|e| Self::error(ActionErrorKind::Write(PathBuf::from(daemon_file), e)))?;
        }

        configure_init_service
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

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ConfigureDeterminateNixDaemonServiceError {}

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

fn generate_plist() -> DeterminateNixDaemonPlist {
    DeterminateNixDaemonPlist {
        keep_alive: true,
        run_at_load: true,
        label: "systems.determinate.nix-daemon".into(),
        program: "/usr/local/bin/determinate-nixd".into(),
        standard_error_path: "/var/log/determinate-nix-daemon.log".into(),
        standard_out_path: "/var/log/determinate-nix-daemon.log".into(),
        soft_resource_limits: ResourceLimits {
            number_of_files: 1048576,
        },
    }
}
