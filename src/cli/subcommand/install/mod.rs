mod determinate;

use std::{
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    cli::{
        ensure_root,
        interaction::{self, PromptChoice},
        signal_channel,
        subcommand::split_receipt::{PHASE1_RECEIPT_LOCATION, PHASE2_RECEIPT_LOCATION},
        CommandExecute,
    },
    error::HasExpectedErrors,
    plan::RECEIPT_LOCATION,
    settings::CommonSettings,
    util::OnMissing,
    BuiltinPlanner, InstallPlan, NixInstallerError,
};
use clap::{ArgAction, Parser};
use color_eyre::{
    eyre::{eyre, WrapErr},
    Section,
};
use owo_colors::OwoColorize;

const EXISTING_INCOMPATIBLE_PLAN_GUIDANCE: &str = "\
    If you are trying to upgrade Nix, try running `sudo -i nix upgrade-nix` instead.\n\
    If you are trying to install Nix over an existing install (from an incompatible `nix-installer` install), try running `/nix/nix-installer uninstall` then try to install again.\n\
    If you are using `nix-installer` in an automated curing process and seeing this message, consider pinning the version you use via https://github.com/NixOS/experimental-nix-installer#accessing-other-versions.\
";

/**
Install Nix using a planner

By default, an appropriate planner is heuristically determined based on the system.

Some planners have additional options which can be set from the planner's subcommand.
*/
#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Install {
    /// Run installation without requiring explicit user confirmation
    #[clap(
        long,
        env = "NIX_INSTALLER_NO_CONFIRM",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub no_confirm: bool,

    #[clap(flatten)]
    pub settings: CommonSettings,

    /// Provide an explanation of the changes the installation process will make to your system
    #[clap(
        long,
        env = "NIX_INSTALLER_EXPLAIN",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub explain: bool,

    /// A path to a non-default installer plan
    #[clap(env = "NIX_INSTALLER_PLAN")]
    pub plan: Option<PathBuf>,

    #[clap(subcommand)]
    pub planner: Option<BuiltinPlanner>,
}

#[async_trait::async_trait]
impl CommandExecute for Install {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute<T>(self, mut feedback: T) -> eyre::Result<ExitCode>
    where
        T: crate::feedback::Feedback,
    {
        let Self {
            no_confirm,
            plan,
            planner: maybe_planner,
            settings,
            explain,
        } = self;

        ensure_root()?;

        let existing_receipt: Option<InstallPlan> = match Path::new(RECEIPT_LOCATION).exists() {
            true => {
                tracing::trace!("Reading existing receipt");
                let install_plan_string = tokio::fs::read_to_string(&RECEIPT_LOCATION)
                    .await
                    .wrap_err("Reading plan")?;
                Some(
                    serde_json::from_str(&install_plan_string).wrap_err_with(|| {
                        format!("Unable to parse existing receipt `{RECEIPT_LOCATION}`, it may be from an incompatible version of `nix-installer`. Try running `/nix/nix-installer uninstall`, then installing again.")
                    })?,
                )
            },
            false => None,
        };

        let uninstall_command = match Path::new("/nix/nix-installer").exists() {
            true => "/nix/nix-installer uninstall".into(),
            false => format!("curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix/tag/v{} | sh -s -- uninstall", env!("CARGO_PKG_VERSION")),
        };

        if plan.is_some() && maybe_planner.is_some() {
            return Err(eyre!("`--plan` conflicts with passing a planner, a planner creates plans, so passing an existing plan doesn't make sense"));
        }

        determinate::inform_macos_about_pkg(&feedback).await;

        let mut post_install_message = None;

        let mut install_plan = if let Some(plan_path) = plan {
            let install_plan_string = tokio::fs::read_to_string(&plan_path)
                .await
                .wrap_err("Reading plan")?;
            serde_json::from_str(&install_plan_string)?
        } else {
            let mut planner = match maybe_planner {
                Some(planner) => planner,
                None => BuiltinPlanner::from_common_settings(settings.clone())
                    .await
                    .map_err(|e| eyre::eyre!(e))?,
            };

            if let Some(existing_receipt) = existing_receipt {
                if let Err(e) = existing_receipt.check_compatible() {
                    eprintln!(
                        "{}",
                        format!("\
                            {e}\n\
                            \n\
                            Found existing plan in `{RECEIPT_LOCATION}` which was created by a version incompatible `nix-installer`.\n\
                            {EXISTING_INCOMPATIBLE_PLAN_GUIDANCE}\n\
                        ").red()
                        );
                    return Ok(ExitCode::FAILURE);
                }

                if existing_receipt.planner.typetag_name() != planner.typetag_name() {
                    eprintln!("{}", format!("Found existing plan in `{RECEIPT_LOCATION}` which used a different planner, try uninstalling the existing install with `{uninstall_command}`").red());
                    return Ok(ExitCode::FAILURE);
                }

                if existing_receipt.planner.settings().map_err(|e| eyre!(e))?
                    != planner.settings().map_err(|e| eyre!(e))?
                {
                    eprintln!("{}", format!("Found existing plan in `{RECEIPT_LOCATION}` which used different planner settings, try uninstalling the existing install with `{uninstall_command}`").red());
                    return Ok(ExitCode::FAILURE);
                }

                eprintln!("{}", format!("Found existing plan in `{RECEIPT_LOCATION}`, with the same settings, already completed. Try uninstalling (`{uninstall_command}`) and reinstalling if Nix isn't working").red());
                return Ok(ExitCode::SUCCESS);
            }

            post_install_message =
                determinate::prompt_for_determinate(&mut feedback, &mut planner, no_confirm)
                    .await?;

            feedback.set_planner(&planner).await?;

            let res = planner.plan().await;
            match res {
                Ok(plan) => plan,
                Err(err) => {
                    feedback.planning_failed(&err).await;
                    if let Some(expected) = err.expected() {
                        eprintln!("{}", expected.red());
                        return Ok(ExitCode::FAILURE);
                    }
                    return Err(err)?;
                },
            }
        };

        feedback.planning_succeeded().await;

        if let Err(err) = install_plan.pre_install_check().await {
            if let Some(expected) = err.expected() {
                eprintln!("{}", expected.red());
                return Ok(ExitCode::FAILURE);
            }
            Err(err)?
        }

        if !no_confirm {
            let mut currently_explaining = explain;
            loop {
                match interaction::prompt(
                    install_plan
                        .describe_install(currently_explaining)
                        .await
                        .map_err(|e| eyre!(e))?,
                    PromptChoice::Yes,
                    currently_explaining,
                )
                .await?
                {
                    PromptChoice::Yes => break,
                    PromptChoice::Explain => currently_explaining = true,
                    PromptChoice::No => {
                        interaction::clean_exit_with_message(
                            "Okay, not continuing with the installation. Bye!",
                        )
                        .await
                    },
                }
            }
        }

        let (tx, rx1) = signal_channel().await?;

        match install_plan.install(feedback.clone(), rx1).await {
            Err(err) => {
                // Attempt to copy self to the store if possible, but since the install failed, this might not work, that's ok.
                copy_self_to_nix_dir().await.ok();

                if !no_confirm {
                    let mut was_expected = false;
                    if let Some(expected) = err.expected() {
                        was_expected = true;
                        eprintln!("{}", expected.red())
                    }

                    let was_cancelled = matches!(err, NixInstallerError::Cancelled);
                    if was_cancelled {
                        eprintln!("{}", err.red());
                    }

                    if !was_expected && !was_cancelled {
                        let error = eyre!(err).wrap_err("Install failure");
                        tracing::error!("{:?}", error);
                    };

                    eprintln!("{}", "Installation failure, offering to revert...".red());
                    let mut currently_explaining = explain;
                    loop {
                        match interaction::prompt(
                            install_plan
                                .describe_uninstall(currently_explaining)
                                .await
                                .map_err(|e| eyre!(e))?,
                            PromptChoice::Yes,
                            currently_explaining,
                        )
                        .await?
                        {
                            PromptChoice::Yes => break,
                            PromptChoice::Explain => currently_explaining = true,
                            PromptChoice::No => {
                                interaction::clean_exit_with_message(
                                    "Okay, didn't do anything! Bye!",
                                )
                                .await
                            },
                        }
                    }
                    let rx2 = tx.subscribe();
                    let res = install_plan.uninstall(feedback, rx2).await;

                    match res {
                        Err(NixInstallerError::ActionRevert(errs)) => {
                            let mut report = eyre!("Multiple errors");
                            for err in errs {
                                report = report.error(err);
                            }
                            return Err(report)?;
                        },
                        Err(err) => {
                            if let Some(expected) = err.expected() {
                                eprintln!("{}", expected.red());
                                return Ok(ExitCode::FAILURE);
                            }
                            if matches!(err, NixInstallerError::Cancelled) {
                                eprintln!("{}", err.red());
                                return Ok(ExitCode::FAILURE);
                            }
                            return Err(err)?;
                        },
                        _ => {
                            println!(
                                "\
                                {message}\n\
                                ",
                                message =
                                    "Partial Nix install was uninstalled successfully!".bold(),
                            );
                        },
                    }
                } else {
                    if let Some(expected) = err.expected() {
                        eprintln!("{}", expected.red());
                        return Ok(ExitCode::FAILURE);
                    }
                    if matches!(err, NixInstallerError::Cancelled) {
                        eprintln!("{}", err.red());
                        return Ok(ExitCode::FAILURE);
                    }

                    let error = eyre!(err).wrap_err("Install failure");
                    return Err(error)?;
                }
            },
            Ok(_) => {
                copy_self_to_nix_dir()
                    .await
                    .wrap_err("Copying `nix-installer` to `/nix/nix-installer`")?;

                let phase1_receipt_path = Path::new(PHASE1_RECEIPT_LOCATION);
                if phase1_receipt_path.exists() {
                    tracing::debug!("Removing pre-existing uninstall phase 1 receipt at {PHASE1_RECEIPT_LOCATION} after successful install");
                    crate::util::remove_file(phase1_receipt_path, OnMissing::Ignore)
                        .await
                        .wrap_err_with(|| format!("Failed to remove uninstall phase 1 receipt at {PHASE1_RECEIPT_LOCATION}"))?;
                }

                let phase2_receipt_path = Path::new(PHASE2_RECEIPT_LOCATION);
                if phase2_receipt_path.exists() {
                    tracing::debug!("Removing pre-existing uninstall phase 2 receipt at {PHASE2_RECEIPT_LOCATION} after successful install");
                    crate::util::remove_file(phase2_receipt_path, OnMissing::Ignore)
                        .await
                        .wrap_err_with(|| format!("Failed to remove uninstall phase 2 receipt at {PHASE2_RECEIPT_LOCATION}"))?;
                }

                println!(
                    "\
                    {success}\n\
                    To get started using Nix, open a new shell or run `{shell_reminder}`\n\
                    ",
                    success = "Nix was installed successfully!".green().bold(),
                    shell_reminder = match std::env::var("SHELL") {
                        Ok(val) if val.contains("fish") =>
                            ". /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.fish".bold(),
                        Ok(_) | Err(_) =>
                            ". /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh".bold(),
                    },
                );

                if let Some(msg) = post_install_message {
                    println!("{}\n", msg.trim());
                }
            },
        }

        Ok(ExitCode::SUCCESS)
    }
}

#[tracing::instrument(level = "debug")]
async fn copy_self_to_nix_dir() -> Result<(), std::io::Error> {
    let path = std::env::current_exe()?;
    tokio::fs::copy(path, "/nix/nix-installer").await?;
    tokio::fs::set_permissions("/nix/nix-installer", PermissionsExt::from_mode(0o0755)).await?;
    Ok(())
}
