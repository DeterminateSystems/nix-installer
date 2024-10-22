use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, Parser};
use eyre::Context as _;
use owo_colors::OwoColorize as _;
use target_lexicon::OperatingSystem;

use crate::action::common::{ConfigureDeterminateNixdInitService, ProvisionDeterminateNixd};
use crate::action::linux::provision_selinux::DETERMINATE_SELINUX_POLICY_PP_CONTENT;
use crate::action::linux::ProvisionSelinux;
use crate::action::{
    Action, ActionDescription, ActionError, ActionState, ActionTag, StatefulAction,
};
use crate::cli::interaction::PromptChoice;
use crate::cli::{ensure_root, CommandExecute};
use crate::error::HasExpectedErrors as _;
use crate::plan::{BINARY_LOCATION, RECEIPT_LOCATION};
use crate::planner::linux::FHS_SELINUX_POLICY_PATH;
use crate::planner::{Planner, PlannerError};
use crate::settings::{CommonSettings, InitSystem, InstallSettingsError};
use crate::InstallPlan;

pub(crate) const ORIGINAL_RECEIPT_LOCATION: &str = "/nix/original-receipt.json";
pub(crate) const ORIGINAL_INSTALLER_BINARY_LOCATION: &str = "/nix/original-nix-installer";
pub(crate) const MAKE_DETERMINATE_BINARY_LOCATION: &str = "/nix/make-determinate-nix-installer";
pub(crate) const MAKE_DETERMINATE_RECEIPT_LOCATION: &str = "/nix/make-determinate-receipt.json";

/**
Make an existing Nix installation into a Determinate Nix installation.
*/
#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true)]
pub struct MakeDeterminate {
    #[clap(
        long,
        env = "NIX_INSTALLER_NO_CONFIRM",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub no_confirm: bool,

    /// Provide an explanation of the changes the installation process will make to your system
    #[clap(
        long,
        env = "NIX_INSTALLER_EXPLAIN",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub explain: bool,
}

#[async_trait::async_trait]
impl CommandExecute for MakeDeterminate {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute(self) -> eyre::Result<ExitCode> {
        ensure_root()?;

        let planner = MakeDeterminatePlanner::default().await?.boxed();
        let mut install_plan = InstallPlan {
            version: crate::plan::current_version()?,
            actions: planner.plan().await?,
            #[cfg(feature = "diagnostics")]
            diagnostic_data: Some(planner.diagnostic_data().await?),
            planner,
        };

        if let Err(err) = install_plan.pre_install_check().await {
            if let Some(expected) = err.expected() {
                eprintln!("{}", expected.red());
                return Ok(ExitCode::FAILURE);
            }
            return Err(err)?;
        }

        if !self.no_confirm {
            let mut currently_explaining = self.explain;
            loop {
                match crate::cli::interaction::prompt(
                    install_plan
                        .describe_install(currently_explaining)
                        .await
                        .map_err(|e| eyre::eyre!(e))?,
                    PromptChoice::Yes,
                    currently_explaining,
                )
                .await?
                {
                    PromptChoice::Yes => break,
                    PromptChoice::Explain => currently_explaining = true,
                    PromptChoice::No => {
                        crate::cli::interaction::clean_exit_with_message(
                            "Okay, didn't do anything! Bye!",
                        )
                        .await
                    },
                }
            }
        }

        for action in install_plan.actions.iter_mut() {
            if let Err(err) = action.try_execute().await {
                println!("{:#?}", err);
                return Ok(ExitCode::FAILURE);
            }
            action.state = ActionState::Completed;
        }

        let receipt_location = PathBuf::from(RECEIPT_LOCATION);
        let make_determinate_receipt_location = PathBuf::from(MAKE_DETERMINATE_RECEIPT_LOCATION);
        let nix_installer_location = PathBuf::from(BINARY_LOCATION);

        if receipt_location.exists() {
            tokio::fs::rename(&receipt_location, ORIGINAL_RECEIPT_LOCATION).await?;
            tracing::info!("Moved original receipt to {ORIGINAL_RECEIPT_LOCATION}");
        }

        if nix_installer_location.exists() {
            tokio::fs::rename(&nix_installer_location, ORIGINAL_INSTALLER_BINARY_LOCATION).await?;
            tracing::info!(
                "Moved original nix-installer binary to {ORIGINAL_INSTALLER_BINARY_LOCATION}"
            );
        }

        install_plan
            .write_receipt(&make_determinate_receipt_location)
            .await
            .wrap_err_with(|| {
                format!("Writing receipt to {make_determinate_receipt_location:?}")
            })?;
        tokio::fs::symlink(&make_determinate_receipt_location, &receipt_location)
            .await
            .wrap_err_with(|| format!("Symlinking Determinate receipt to {receipt_location:?}"))?;
        tracing::info!("Wrote Determinate receipt");

        let current_exe = std::env::current_exe()?;
        tokio::fs::copy(&current_exe, MAKE_DETERMINATE_BINARY_LOCATION)
            .await
            .wrap_err_with(|| {
                format!("Copying {current_exe:?} to {MAKE_DETERMINATE_BINARY_LOCATION}")
            })?;
        tokio::fs::set_permissions(
            MAKE_DETERMINATE_BINARY_LOCATION,
            PermissionsExt::from_mode(0o0755),
        )
        .await
        .wrap_err_with(|| {
            format!("Setting {MAKE_DETERMINATE_BINARY_LOCATION} permissions to 755")
        })?;
        tokio::fs::symlink(MAKE_DETERMINATE_BINARY_LOCATION, &nix_installer_location)
            .await
            .wrap_err_with(|| {
                format!(
                    "Symlinking {MAKE_DETERMINATE_BINARY_LOCATION} to {nix_installer_location:?}"
                )
            })?;

        tracing::info!("Finished preparing successfully!");

        Ok(ExitCode::SUCCESS)
    }
}

/// A planner for making existing Nix installations into Determinate Nix installations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MakeDeterminatePlanner {
    pub settings: CommonSettings,
}

#[async_trait::async_trait]
#[typetag::serde(name = "make_determinate")]
impl Planner for MakeDeterminatePlanner {
    async fn default() -> Result<Self, PlannerError> {
        Ok(Self {
            settings: CommonSettings::default().await?,
        })
    }

    async fn plan(&self) -> Result<Vec<StatefulAction<Box<dyn Action>>>, PlannerError> {
        // NOTE(cole-h): add a cosmetic revert note to the list of "planned actions" when reverting;
        // this way the user will not be quite as shocked when they go to uninstall and see only
        // dnixd is being uninstalled: we will uninstall with the original installer binary after we
        // uninstall dnixd
        let cosmetic_revert_note =
            CosmeticRevertNote::plan(String::from("Execute the original Nix uninstaller"))
                .await
                .map_err(PlannerError::Action)?
                .boxed();

        let mut plan = vec![cosmetic_revert_note];

        let host = OperatingSystem::host();

        match host {
            OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => {
                // Nothing macOS-specific at this point
            },
            _ => {
                let has_selinux = crate::planner::linux::detect_selinux().await?;
                if has_selinux {
                    plan.push(
                        ProvisionSelinux::plan(
                            FHS_SELINUX_POLICY_PATH.into(),
                            DETERMINATE_SELINUX_POLICY_PP_CONTENT,
                        )
                        .await
                        .map_err(PlannerError::Action)?
                        .boxed(),
                    );
                }
            },
        }

        plan.push(
            ProvisionDeterminateNixd::plan()
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );

        let init = match host {
            OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => InitSystem::Launchd,
            _ => InitSystem::Systemd,
        };
        plan.push(
            ConfigureDeterminateNixdInitService::plan(init, true)
                .await
                .map_err(PlannerError::Action)?
                .boxed(),
        );

        Ok(plan)
    }

    fn settings(&self) -> Result<HashMap<String, serde_json::Value>, InstallSettingsError> {
        let Self { settings } = self;
        let mut map = HashMap::default();

        map.extend(settings.settings()?);

        Ok(map)
    }

    async fn configured_settings(
        &self,
    ) -> Result<HashMap<String, serde_json::Value>, PlannerError> {
        let default = Self::default().await?.settings()?;
        let configured = self.settings()?;

        let mut settings: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, value) in configured.iter() {
            if default.get(key) != Some(value) {
                settings.insert(key.clone(), value.clone());
            }
        }

        Ok(settings)
    }

    #[cfg(feature = "diagnostics")]
    async fn diagnostic_data(&self) -> Result<crate::diagnostics::DiagnosticData, PlannerError> {
        Ok(crate::diagnostics::DiagnosticData::new(
            self.settings.diagnostic_attribution.clone(),
            self.settings.diagnostic_endpoint.clone(),
            self.typetag_name().into(),
            self.configured_settings()
                .await?
                .into_keys()
                .collect::<Vec<_>>(),
            self.settings.ssl_cert_file.clone(),
        )?)
    }

    async fn platform_check(&self) -> Result<(), PlannerError> {
        match OperatingSystem::host() {
            OperatingSystem::Linux | OperatingSystem::MacOSX { .. } | OperatingSystem::Darwin => {
                Ok(())
            },
            host_os => Err(PlannerError::IncompatibleOperatingSystem {
                planner: self.typetag_name(),
                host_os,
            }),
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "cosmetic_revert_note")]
struct CosmeticRevertNote {
    note: String,
}

impl CosmeticRevertNote {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(note: String) -> Result<StatefulAction<Self>, ActionError> {
        Ok(StatefulAction::completed(Self { note }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "cosmetic_revert_note")]
impl Action for CosmeticRevertNote {
    fn action_tag() -> ActionTag {
        ActionTag("cosmetic_revert_note")
    }

    fn tracing_synopsis(&self) -> String {
        String::new()
    }

    fn tracing_span(&self) -> tracing::Span {
        tracing::span!(tracing::Level::TRACE, "cosmetic_revert_note")
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        Vec::new()
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.note.clone(), vec![])]
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        Ok(())
    }
}
