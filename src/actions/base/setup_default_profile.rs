use crate::{HarmonicError, execute_command};

use glob::glob;
use tokio::process::Command;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SetupDefaultProfile {
    channels: Vec<String>,
}

impl SetupDefaultProfile {
    pub async fn plan(channels: Vec<String>) -> Result<Self, HarmonicError> {
        Ok(Self { channels })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for SetupDefaultProfile {
    type Receipt = SetupDefaultProfileReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Setup the default Nix profile".to_string(),
                vec![
                    "TODO".to_string()
                ]
            ),
        ]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { channels } = self;
        tracing::info!("Setting up default profile");

        // Find an `nix` package
        let nix_pkg_glob = "/nix/store/*-nix-*";
        let mut found_nix_pkg = None;
        for entry in glob(nix_pkg_glob).map_err(HarmonicError::GlobPatternError)? {
            match entry {
                Ok(path) => {
                    // TODO(@Hoverbear): Should probably ensure is unique
                    found_nix_pkg = Some(path);
                    break;
                },
                Err(_) => continue, /* Ignore it */
            };
        }
        let nix_pkg = if let Some(nix_pkg) = found_nix_pkg {
            nix_pkg
        } else {
            return Err(HarmonicError::NoNssCacert); // TODO(@hoverbear): Fix this error
        };

        // Install `nix` itself into the store
        execute_command(
            Command::new(nix_pkg.join("bin/nix-env"))
                .arg("-i")
                .arg(&nix_pkg),
            false,
        )
        .await?;

        // Find an `nss-cacert` package, add it too.
        let nss_ca_cert_pkg_glob = "/nix/store/*-nss-cacert-*";
        let mut found_nss_ca_cert_pkg = None;
        for entry in glob(nss_ca_cert_pkg_glob).map_err(HarmonicError::GlobPatternError)? {
            match entry {
                Ok(path) => {
                    // TODO(@Hoverbear): Should probably ensure is unique
                    found_nss_ca_cert_pkg = Some(path);
                    break;
                },
                Err(_) => continue, /* Ignore it */
            };
        };
        let nss_ca_cert_pkg = if let Some(nss_ca_cert_pkg) = found_nss_ca_cert_pkg {
            nss_ca_cert_pkg
        } else {
            return Err(HarmonicError::NoNssCacert);
        };
        
        // Install `nss-cacert` into the store
        execute_command(
            Command::new(nix_pkg.join("bin/nix-env"))
                .arg("-i")
                .arg(&nss_ca_cert_pkg),
            false,
        )
        .await?;

        if !channels.is_empty() {
            let mut command = Command::new(nix_pkg.join("bin/nix-channel"));
            command.arg("--update");
            for channel in channels {
                command.arg(channel);
            }
            command.env("NIX_SSL_CERT_FILE", "/nix/var/nix/profiles/default/etc/ssl/certs/ca-bundle.crt");
            
            let command_str = format!("{:?}", command.as_std());
            let status = command
                .status()
                .await
                .map_err(|e| HarmonicError::CommandFailedExec(command_str.clone(), e))?;
            
            match status.success() {
                true => (),
                false => return Err(HarmonicError::CommandFailedStatus(command_str)),
            }
        }
        Ok(Self::Receipt {})
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct SetupDefaultProfileReceipt {}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for SetupDefaultProfileReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        vec![
            ActionDescription::new(
                "Unset the default Nix profile".to_string(),
                vec![
                    "TODO".to_string()
                ]
            ),
        ]
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
