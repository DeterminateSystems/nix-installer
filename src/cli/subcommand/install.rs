use std::{
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    action::ActionState,
    cli::{
        ensure_root,
        interaction::{self, PromptChoice},
        signal_channel, CommandExecute,
    },
    error::HasExpectedErrors,
    plan::RECEIPT_LOCATION,
    planner::Planner,
    settings::CommonSettings,
    BuiltinPlanner, InstallPlan, NixInstallerError,
};
use clap::{ArgAction, Parser};
use color_eyre::{
    eyre::{eyre, WrapErr},
    Section,
};
use owo_colors::OwoColorize;

const EXISTING_INCOMPATIBLE_PLAN_GUIDANCE: &'static str = "\
    If you are trying to upgrade Nix, try running `sudo -i nix upgrade-nix` instead.\n\
    If you are trying to install Nix over an existing install (from an incompatible `nix-installer` install), try running `/nix/nix-installer uninstall` then try to install again.\n\
    If you are using `nix-installer` in an automated curing process and seeing this message, consider pinning the version you use via https://github.com/DeterminateSystems/nix-installer#accessing-other-versions.\
";

/// Execute an install (possibly using an existing plan)
///
/// To pass custom options, select a planner, for example `nix-installer install linux-multi --help`
#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Install {
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

    #[clap(
        long,
        env = "NIX_INSTALLER_EXPLAIN",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub explain: bool,

    #[clap(env = "NIX_INSTALLER_PLAN")]
    pub plan: Option<PathBuf>,

    #[clap(subcommand)]
    pub planner: Option<BuiltinPlanner>,
}

#[async_trait::async_trait]
impl CommandExecute for Install {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            no_confirm,
            plan,
            planner,
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

        let mut install_plan = match (planner, plan) {
            (Some(planner), None) => {
                let chosen_planner: Box<dyn Planner> = planner.clone().boxed();

                match existing_receipt {
                    Some(existing_receipt) => {
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
                            return Ok(ExitCode::FAILURE)
                        }
                        if existing_receipt.planner.typetag_name() != chosen_planner.typetag_name() {
                            eprintln!("{}", format!("Found existing plan in `{RECEIPT_LOCATION}` which used a different planner, try uninstalling the existing install with `{uninstall_command}`").red());
                            return Ok(ExitCode::FAILURE)
                        }
                        if existing_receipt.planner.settings().map_err(|e| eyre!(e))? != chosen_planner.settings().map_err(|e| eyre!(e))? {
                            eprintln!("{}", format!("Found existing plan in `{RECEIPT_LOCATION}` which used different planner settings, try uninstalling the existing install with `{uninstall_command}`").red());
                            return Ok(ExitCode::FAILURE)
                        }
                        eprintln!("{}", format!("Found existing plan in `{RECEIPT_LOCATION}`, with the same settings, already completed, try uninstalling (`{uninstall_command}`) and reinstalling if Nix isn't working").red());
                        return Ok(ExitCode::FAILURE)
                    },
                    None => {
                        let res = planner.plan().await;
                        match res {
                            Ok(plan) => plan,
                            Err(err) => {
                                if let Some(expected) = err.expected() {
                                    eprintln!("{}", expected.red());
                                    return Ok(ExitCode::FAILURE);
                                }
                                return Err(err)?;
                            }
                        }
                    },
                }
            },
            (None, Some(plan_path)) => {
                let install_plan_string = tokio::fs::read_to_string(&plan_path)
                .await
                .wrap_err("Reading plan")?;
                serde_json::from_str(&install_plan_string)?
            },
            (None, None) => {
                let builtin_planner = BuiltinPlanner::from_common_settings(settings.clone())
                    .await
                    .map_err(|e| eyre::eyre!(e))?;

                match existing_receipt {
                    Some(existing_receipt) => {
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
                            return Ok(ExitCode::FAILURE)
                        }
                        if existing_receipt.planner.typetag_name() != builtin_planner.typetag_name() {
                            eprintln!("{}", format!("Found existing plan in `{RECEIPT_LOCATION}` which used a different planner, try uninstalling the existing install with `{uninstall_command}`").red());
                            return Ok(ExitCode::FAILURE)
                        }
                        if existing_receipt.planner.settings().map_err(|e| eyre!(e))? != builtin_planner.settings().map_err(|e| eyre!(e))? {
                            eprintln!("{}", format!("Found existing plan in `{RECEIPT_LOCATION}` which used different planner settings, try uninstalling the existing install with `{uninstall_command}`").red());
                            return Ok(ExitCode::FAILURE)
                        }
                        if existing_receipt.actions.iter().all(|v| v.state == ActionState::Completed) {
                            eprintln!("{}", format!("Found existing plan in `{RECEIPT_LOCATION}`, with the same settings, already completed, try uninstalling (`{uninstall_command}`) and reinstalling if Nix isn't working").yellow());
                            return Ok(ExitCode::SUCCESS)
                        }
                        existing_receipt
                    },
                    None => {
                        let res = builtin_planner.plan().await;
                        match res {
                            Ok(plan) => plan,
                            Err(err) => {
                                if let Some(expected) = err.expected() {
                                    eprintln!("{}", expected.red());
                                    return Ok(ExitCode::FAILURE);
                                }
                                return Err(err)?;
                            }
                        }
                    },
                }
            },
            (Some(_), Some(_)) => return Err(eyre!("`--plan` conflicts with passing a planner, a planner creates plans, so passing an existing plan doesn't make sense")),
        };

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
                        interaction::clean_exit_with_message("Okay, didn't do anything! Bye!").await
                    },
                }
            }
        }

        let (tx, rx1) = signal_channel().await?;

        match install_plan.install(rx1).await {
            Err(err) => {
                if !no_confirm {
                    // Attempt to copy self to the store if possible, but since the install failed, this might not work, that's ok.
                    copy_self_to_nix_dir().await.ok();

                    let mut was_expected = false;
                    if let Some(expected) = err.expected() {
                        was_expected = true;
                        eprintln!("{}", expected.red())
                    }
                    if !was_expected {
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
                    let res = install_plan.uninstall(rx2).await;

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

                    let error = eyre!(err).wrap_err("Install failure");
                    return Err(error)?;
                }
            },
            Ok(_) => {
                copy_self_to_nix_dir()
                    .await
                    .wrap_err("Copying `nix-installer` to `/nix/nix-installer`")?;
                println!(
                    "\
                    {success}\n\
                    To get started using Nix, open a new shell or run `{maybe_ssl_cert_file_reminder}{shell_reminder}`\n\
                    ",
                    success = "Nix was installed successfully!".green().bold(),
                    shell_reminder = match std::env::var("SHELL") {
                        Ok(val) if val.contains("fish") =>
                            ". /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.fish".bold(),
                        Ok(_) | Err(_) =>
                            ". /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh".bold(),
                    },
                    maybe_ssl_cert_file_reminder = if let Some(ssl_cert_file) = &settings.ssl_cert_file {
                        format!(
                            "export NIX_SSL_CERT_FILE={:?}; ",
                            ssl_cert_file
                                .canonicalize()
                                .map_err(|e| { eyre!(e).wrap_err(format!("Could not canonicalize {}", ssl_cert_file.display())) })?
                        )
                    } else {
                        "".to_string()
                    }
                );
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
