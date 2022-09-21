use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::{HarmonicError, execute_command};

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

const SERVICE_SRC: &str =
"/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.service";

const SOCKET_SRC: &str =
"/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.socket";

const TMPFILES_SRC: &str =
"/nix/var/nix/profiles/default//lib/tmpfiles.d/nix-daemon.conf";
const TMPFILES_DEST: &str = "/etc/tmpfiles.d/nix-daemon.conf";


#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixDaemonService {}

impl ConfigureNixDaemonService {
    pub async fn plan() -> Result<Self, HarmonicError> {
        if !Path::new("/run/systemd/system").exists() {
            return Err(HarmonicError::InitNotSupported);
        }
        Ok(Self {})
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for ConfigureNixDaemonService {
    type Receipt = ConfigureNixDaemonServiceReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Configure the Nix Daemon Service".to_string(),
                vec![
                    "Sets init system specific options".to_string()
                ]
            ),
        ]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        tracing::info!("Configuring nix daemon service");

        tracing::trace!("Symlinking");
        tokio::fs::symlink(TMPFILES_SRC, TMPFILES_DEST)
            .await
            .map_err(|e| HarmonicError::Symlink(PathBuf::from(TMPFILES_SRC), PathBuf::from(TMPFILES_DEST), e))?;
        
        execute_command(
            Command::new("systemd-tmpfiles")
                .arg("--create")
                .arg("--prefix=/nix/var/nix"),
            false,
        ).await?;

        execute_command(
            Command::new("systemctl").arg("link").arg(SERVICE_SRC),
            false,
        )
        .await?;

        execute_command(Command::new("systemctl").arg("daemon-reload"), false).await?;

        Ok(Self::Receipt {})
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ConfigureNixDaemonServiceReceipt {}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for ConfigureNixDaemonServiceReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Stop the systemd Nix daemon".to_string(),
                vec![
                    "The `nix` command line tool communicates with a running Nix daemon managed by your init system".to_string()
                ]
            ),
        ]
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
