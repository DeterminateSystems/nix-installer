use std::process::ExitCode;

use clap::{ArgAction, Parser, Subcommand};
use eyre::Context as _;
use target_lexicon::OperatingSystem;

use crate::action::common::{ConfigureDeterminateNixdInitService, ProvisionDeterminateNixd};
use crate::action::linux::provision_selinux::DETERMINATE_SELINUX_POLICY_PP_CONTENT;
use crate::action::linux::ProvisionSelinux;
use crate::action::ActionState;
use crate::cli::{ensure_root, CommandExecute};
use crate::plan::RECEIPT_LOCATION;
use crate::planner::PlannerError;
use crate::settings::InitSystem;
use crate::InstallPlan;

/**
FIXME

FIXME: also regenerate the fixtures

FIXME: test how the receipt thing works with older installer versions
*/
#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true)]
// FIXME: make-determinate instead
pub struct Prepare {
    #[clap(
        long,
        env = "NIX_INSTALLER_NO_CONFIRM",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub no_confirm: bool,

    #[command(subcommand)]
    command: Option<PrepareKind>,
}

#[derive(Clone, Debug, Subcommand, serde::Deserialize, serde::Serialize)]
pub enum PrepareKind {
    /// Prepare the system to upgrade to Determinate Nix.
    Determinate,
}

impl Prepare {
    pub fn command(&self) -> PrepareKind {
        self.command.to_owned().unwrap_or(PrepareKind::Determinate)
    }
}

#[async_trait::async_trait]
impl CommandExecute for Prepare {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute(self) -> eyre::Result<ExitCode> {
        ensure_root()?;

        let command = self.command();

        let mut plan = Vec::new();

        match command {
            PrepareKind::Determinate => {
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
                                    "/usr/share/selinux/packages/nix.pp".into(),
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
            },
        }

        // TODO: summarize all the stuff that's gonna happen, take inspiration from repair command

        for action in plan.iter_mut() {
            if let Err(err) = action.try_execute().await {
                println!("{:#?}", err);
                return Ok(ExitCode::FAILURE);
            }
            action.state = ActionState::Completed;
        }

        if std::path::Path::new(RECEIPT_LOCATION).exists() {
            tracing::trace!("Reading existing receipt");
            let install_plan_string = tokio::fs::read_to_string(&RECEIPT_LOCATION)
                .await
                .wrap_err("Reading plan")?;
            let mut existing_receipt: InstallPlan = serde_json::from_str(&install_plan_string)
                .wrap_err_with(|| {
                    format!(
                        "Unable to parse existing receipt `{RECEIPT_LOCATION}`, \
                        it may be from an incompatible version of `nix-installer`. \
                        Try running `/nix/nix-installer uninstall`, then installing again."
                    )
                })?;

            existing_receipt.actions.extend(plan);
            existing_receipt.write_receipt().await?;
            tracing::info!("Wrote updated receipt");
        }

        tracing::info!("Finished preparing successfully!");

        Ok(ExitCode::SUCCESS)
    }
}
