use std::{
    ffi::CString,
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    cli::{ensure_root, interaction::PromptChoice, signal_channel},
    error::HasExpectedErrors,
    plan::{current_version, RECEIPT_LOCATION},
    InstallPlan, NixInstallerError,
};
use clap::{ArgAction, Parser};
use color_eyre::eyre::{eyre, WrapErr};
use owo_colors::OwoColorize;
use rand::Rng;

use crate::cli::{interaction, CommandExecute};

/// Uninstall a previously `nix-installer` installed Nix
#[derive(Debug, Parser)]
pub struct Uninstall {
    #[clap(
        long,
        env = "NIX_INSTALLER_NO_CONFIRM",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub no_confirm: bool,

    #[clap(
        long,
        env = "NIX_INSTALLER_EXPLAIN",
        action(ArgAction::SetTrue),
        default_value = "false",
        global = true
    )]
    pub explain: bool,

    #[clap(default_value = RECEIPT_LOCATION)]
    pub receipt: PathBuf,
}

#[async_trait::async_trait]
impl CommandExecute for Uninstall {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self {
            no_confirm,
            receipt,
            explain,
        } = self;

        ensure_root()?;

        if let Ok(current_dir) = std::env::current_dir() {
            let mut components = current_dir.components();
            let should_be_root = components.next();
            let maybe_nix = components.next();
            if should_be_root == Some(std::path::Component::RootDir)
                && maybe_nix == Some(std::path::Component::Normal(std::ffi::OsStr::new("nix")))
            {
                tracing::debug!("Changing current directory to be outside of `/nix`");
                std::env::set_current_dir("/").wrap_err("Uninstall process was run from `/nix` folder, but could not change directory away from `/nix`, please change the current directory and try again.")?;
            }
        }

        // During install, `nix-installer` will store a copy of itself in `/nix/nix-installer`
        // If the user opted to run that particular copy of `nix-installer` to do this uninstall,
        // well, we have a problem, since the binary would delete itself.
        // Instead, detect if we're in that location, if so, move the binary and `execv` it.
        if let Ok(current_exe) = std::env::current_exe() {
            if current_exe.as_path() == Path::new("/nix/nix-installer") {
                tracing::debug!(
                    "Detected uninstall from `/nix/nix-installer`, moving executable and re-executing"
                );
                let temp = std::env::temp_dir();
                let random_trailer: String = {
                    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                        abcdefghijklmnopqrstuvwxyz\
                                            0123456789";
                    const PASSWORD_LEN: usize = 16;
                    let mut rng = rand::thread_rng();

                    (0..PASSWORD_LEN)
                        .map(|_| {
                            let idx = rng.gen_range(0..CHARSET.len());
                            CHARSET[idx] as char
                        })
                        .collect()
                };
                let temp_exe = temp.join(format!("nix-installer-{random_trailer}"));
                tokio::fs::copy(&current_exe, &temp_exe)
                    .await
                    .wrap_err("Copying nix-installer to tempdir")?;
                let args = std::env::args();
                let mut arg_vec_cstring = vec![];
                for arg in args {
                    arg_vec_cstring.push(CString::new(arg).wrap_err("Making arg into C string")?);
                }
                let temp_exe_cstring = CString::new(temp_exe.to_string_lossy().into_owned())
                    .wrap_err("Making C string of executable path")?;

                tracing::trace!("Execv'ing `{temp_exe_cstring:?} {arg_vec_cstring:?}`");
                nix::unistd::execv(&temp_exe_cstring, &arg_vec_cstring)
                    .wrap_err("Executing copied `nix-installer`")?;
            }
        }

        let install_receipt_string = tokio::fs::read_to_string(receipt)
            .await
            .wrap_err("Reading receipt")?;

        let mut plan: InstallPlan = match serde_json::from_str(&install_receipt_string) {
            Ok(plan) => plan,
            Err(plan_err) => {
                #[derive(serde::Deserialize)]
                struct MinimalPlan {
                    version: semver::Version,
                }
                let minimal_plan: Result<MinimalPlan, _> =
                    serde_json::from_str(&install_receipt_string);
                match minimal_plan {
                    Ok(minimal_plan) => {
                        return Err(plan_err).wrap_err_with(|| {
                            let plan_version = minimal_plan.version;
                            let current_version = current_version().map(|v| v.to_string()).unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
                            format!(
                            "\
                            Unable to parse plan, this plan was created by `nix-installer` version `{plan_version}`, this is `nix-installer` version `{current_version}`\n\
                            To uninstall, either run  `/nix/nix-installer uninstall` or `curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix/tag/v{plan_version} | sh -s -- uninstall`\
                            ").red().to_string()
                        });
                    },
                    Err(_minimal_plan_err) => return Err(plan_err)?,
                }
            },
        };

        if let Err(e) = plan.check_compatible() {
            let version = plan.version;
            eprintln!(
                "{}", 
                format!("\
                    {e}\n\
                    \n\
                    Found existing plan in `{RECEIPT_LOCATION}` which was created by a version incompatible `nix-installer`.\n\
                    \n
                    To uninstall, either run `/nix/nix-installer uninstall` or `curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix/tag/v${version} | sh -s -- uninstall`\n\
                    \n\
                ").red()
            );
            return Ok(ExitCode::FAILURE);
        }

        if let Err(err) = plan.pre_uninstall_check().await {
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
                    plan.describe_uninstall(currently_explaining)
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

        let (_tx, rx) = signal_channel().await?;

        let res = plan.uninstall(rx).await;
        match res {
            Err(err @ NixInstallerError::ActionRevert(_)) => {
                tracing::error!("Uninstallation complete, some errors encountered");
                return Err(err)?;
            },
            Err(err) => {
                if let Some(expected) = err.expected() {
                    println!("{}", expected.red());
                    return Ok(ExitCode::FAILURE);
                }
                return Err(err)?;
            },
            _ => (),
        }

        println!(
            "\
            {success}\n\
            ",
            success = "Nix was uninstalled successfully!".green().bold(),
        );

        Ok(ExitCode::SUCCESS)
    }
}
