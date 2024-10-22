use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, Parser};
use eyre::Context as _;
use owo_colors::OwoColorize as _;
use target_lexicon::OperatingSystem;

use crate::action::common::{ConfigureDeterminateNixdInitService, ProvisionDeterminateNixd};
use crate::action::linux::provision_selinux::DETERMINATE_SELINUX_POLICY_PP_CONTENT;
use crate::action::linux::ProvisionSelinux;
use crate::action::ActionState;
use crate::cli::interaction::PromptChoice;
use crate::cli::{ensure_root, CommandExecute};
use crate::error::HasExpectedErrors as _;
use crate::plan::RECEIPT_LOCATION;
use crate::planner::linux::FHS_SELINUX_POLICY_PATH;
use crate::planner::PlannerError;
use crate::settings::InitSystem;
use crate::InstallPlan;

pub(crate) const ORIGINAL_RECEIPT_LOCATION: &str = "/nix/original-receipt.json";
pub(crate) const ORIGINAL_INSTALLER_BINARY_LOCATION: &str = "/nix/original-nix-installer";

/**
FIXME

FIXME: also regenerate the fixtures

FIXME: test how the receipt thing works with older installer versions
it doesn't, so we need to write the receipt somewhere else
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

        let mut plan = Vec::new();

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

        // TODO: make "make-determinate" planner?
        let mut install_plan = InstallPlan::default().await?;
        install_plan.actions = plan;

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
        let nix_installer_location = PathBuf::from("/nix/nix-installer");

        if receipt_location.exists() {
            tokio::fs::copy(&receipt_location, ORIGINAL_RECEIPT_LOCATION).await?;
            tracing::info!("Copied original receipt to {ORIGINAL_RECEIPT_LOCATION}");
        }

        if nix_installer_location.exists() {
            tokio::fs::copy(nix_installer_location, ORIGINAL_INSTALLER_BINARY_LOCATION).await?;
            tracing::info!(
                "Copied original nix-installer binary to {ORIGINAL_INSTALLER_BINARY_LOCATION}"
            );
        }

        install_plan.write_receipt(receipt_location).await?;
        tracing::info!("Wrote Determinate receipt");

        crate::cli::subcommand::install::copy_self_to_nix_dir()
            .await
            .wrap_err("Copying `nix-installer make-determinate` to `/nix/nix-installer`")?;

        tracing::info!("Finished preparing successfully!");

        Ok(ExitCode::SUCCESS)
    }
}
