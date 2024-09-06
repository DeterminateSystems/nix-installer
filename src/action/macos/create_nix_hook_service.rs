use serde::{Deserialize, Serialize};
use tracing::{span, Span};

use std::path::PathBuf;
use tokio::{
    fs::{remove_file, OpenOptions},
    io::AsyncWriteExt,
    process::Command,
};

use crate::action::{
    Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
};

use super::DARWIN_LAUNCHD_DOMAIN;

/** Create a plist for a `launchctl` service to re-add Nix to the zshrc after upgrades.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_nix_hook_service")]
pub struct CreateNixHookService {
    path: PathBuf,
    service_label: String,
    needs_bootout: bool,
}

impl CreateNixHookService {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let mut this = Self {
            path: PathBuf::from(
                "/Library/LaunchDaemons/systems.determinate.nix-installer.nix-hook.plist",
            ),
            service_label: "systems.determinate.nix-installer.nix-hook".into(),
            needs_bootout: false,
        };

        // If the service is currently loaded or running, we need to unload it during execute (since we will then recreate it and reload it)
        // This `launchctl` command may fail if the service isn't loaded
        let mut check_loaded_command = Command::new("launchctl");
        check_loaded_command.process_group(0);
        check_loaded_command.arg("print");
        check_loaded_command.arg(format!("system/{}", this.service_label));
        tracing::trace!(
            command = format!("{:?}", check_loaded_command.as_std()),
            "Executing"
        );
        let check_loaded_output = check_loaded_command
            .output()
            .await
            .map_err(|e| ActionErrorKind::command(&check_loaded_command, e))
            .map_err(Self::error)?;
        this.needs_bootout = check_loaded_output.status.success();
        if this.needs_bootout {
            tracing::debug!(
                "Detected loaded service `{}` which needs unload before replacing `{}`",
                this.service_label,
                this.path.display(),
            );
        }

        if this.path.exists() {
            let discovered_plist: LaunchctlHookPlist =
                plist::from_file(&this.path).map_err(Self::error)?;
            let expected_plist = generate_plist(&this.service_label)
                .await
                .map_err(Self::error)?;
            if discovered_plist != expected_plist {
                tracing::trace!(
                    ?discovered_plist,
                    ?expected_plist,
                    "Parsed plists not equal"
                );
                return Err(Self::error(CreateNixHookServiceError::DifferentPlist {
                    expected: expected_plist,
                    discovered: discovered_plist,
                    path: this.path.clone(),
                }));
            }

            tracing::debug!("Creating file `{}` already complete", this.path.display());
            return Ok(StatefulAction::completed(this));
        }

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_nix_hook_service")]
impl Action for CreateNixHookService {
    fn action_tag() -> ActionTag {
        ActionTag("create_nix_hook_service")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "{maybe_unload} a `launchctl` plist to put Nix into your PATH",
            maybe_unload = if self.needs_bootout {
                "Unload, then recreate"
            } else {
                "Create"
            }
        )
    }

    fn tracing_span(&self) -> Span {
        let span = span!(
            tracing::Level::DEBUG,
            "create_nix_hook_service",
            path = tracing::field::display(self.path.display()),
            buf = tracing::field::Empty,
        );

        span
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            service_label,
            needs_bootout,
        } = self;

        if *needs_bootout {
            crate::action::macos::retry_bootout(DARWIN_LAUNCHD_DOMAIN, &path)
                .await
                .map_err(Self::error)?;
        }

        let generated_plist = generate_plist(service_label).await.map_err(Self::error)?;

        let mut options = OpenOptions::new();
        options.create(true).write(true).read(true);

        let mut file = options
            .open(&path)
            .await
            .map_err(|e| Self::error(ActionErrorKind::Open(path.to_owned(), e)))?;

        let mut buf = Vec::new();
        plist::to_writer_xml(&mut buf, &generated_plist).map_err(Self::error)?;
        file.write_all(&buf)
            .await
            .map_err(|e| Self::error(ActionErrorKind::Write(path.to_owned(), e)))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Delete file `{}`", self.path.display()),
            vec![format!("Delete file `{}`", self.path.display())],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        remove_file(&self.path)
            .await
            .map_err(|e| Self::error(ActionErrorKind::Remove(self.path.to_owned(), e)))?;

        Ok(())
    }
}

/// This function must be able to operate at both plan and execute time.
async fn generate_plist(service_label: &str) -> Result<LaunchctlHookPlist, ActionErrorKind> {
    let plist = LaunchctlHookPlist {
        keep_alive: KeepAliveOpts {
            successful_exit: false,
        },
        label: service_label.into(),
        program_arguments: vec![
            "/bin/sh".into(),
            "-c".into(),
            "/bin/wait4path /nix/nix-installer && /nix/nix-installer repair".into(),
        ],
        standard_error_path: "/nix/.nix-installer-hook.err.log".into(),
        standard_out_path: "/nix/.nix-installer-hook.out.log".into(),
    };

    Ok(plist)
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct LaunchctlHookPlist {
    label: String,
    program_arguments: Vec<String>,
    keep_alive: KeepAliveOpts,
    standard_error_path: String,
    standard_out_path: String,
}

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct KeepAliveOpts {
    successful_exit: bool,
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum CreateNixHookServiceError {
    #[error(
        "`{path}` exists and contains content different than expected. Consider removing the file."
    )]
    DifferentPlist {
        expected: LaunchctlHookPlist,
        discovered: LaunchctlHookPlist,
        path: PathBuf,
    },
}

impl From<CreateNixHookServiceError> for ActionErrorKind {
    fn from(val: CreateNixHookServiceError) -> Self {
        ActionErrorKind::Custom(Box::new(val))
    }
}
