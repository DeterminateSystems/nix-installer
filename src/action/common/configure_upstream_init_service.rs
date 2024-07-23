use std::path::PathBuf;

use tracing::{span, Span};

use crate::action::{ActionError, ActionTag, StatefulAction};

use crate::action::{common::ConfigureInitService, Action, ActionDescription};
use crate::settings::InitSystem;

// Linux
const SERVICE_SRC: &str = "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.service";
const SERVICE_DEST: &str = "/etc/systemd/system/nix-daemon.service";

// Darwin
const DARWIN_NIX_DAEMON_SOURCE: &str =
    "/nix/var/nix/profiles/default/Library/LaunchDaemons/org.nixos.nix-daemon.plist";
const DARWIN_NIX_DAEMON_DEST: &str = "/Library/LaunchDaemons/org.nixos.nix-daemon.plist";
const DARWIN_LAUNCHD_SERVICE_NAME: &str = "org.nixos.nix-daemon";

/**
Configure the init to run the Nix daemon
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureUpstreamInitService {
    configure_init_service: StatefulAction<ConfigureInitService>,
}

impl ConfigureUpstreamInitService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        init: InitSystem,
        start_daemon: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let service_src: Option<PathBuf> = match init {
            InitSystem::Launchd => Some(DARWIN_NIX_DAEMON_SOURCE.into()),
            InitSystem::Systemd => Some(SERVICE_SRC.into()),
            InitSystem::None => None,
        };
        let service_dest: Option<PathBuf> = match init {
            InitSystem::Launchd => Some(DARWIN_NIX_DAEMON_DEST.into()),
            InitSystem::Systemd => Some(SERVICE_DEST.into()),
            InitSystem::None => None,
        };
        let service_name: Option<String> = match init {
            InitSystem::Launchd => Some(DARWIN_LAUNCHD_SERVICE_NAME.into()),
            _ => None,
        };

        let configure_init_service =
            ConfigureInitService::plan(init, start_daemon, service_src, service_dest, service_name)
                .await
                .map_err(Self::error)?;

        Ok(Self {
            configure_init_service,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_upstream_init_service")]
impl Action for ConfigureUpstreamInitService {
    fn action_tag() -> ActionTag {
        ActionTag("create_upstream_init_service")
    }
    fn tracing_synopsis(&self) -> String {
        "Configure upstream Nix daemon service".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "create_upstream_init_service",)
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
            format!("Remove upstream Nix daemon service",),
            vec![self.configure_init_service.tracing_synopsis()],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        self.configure_init_service.try_revert().await?;

        Ok(())
    }
}
