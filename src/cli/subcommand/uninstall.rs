use std::{ffi::CString, path::PathBuf, process::ExitCode};

use crate::{
    cli::{
        ensure_root,
        interaction::PromptChoice,
        signal_channel,
        subcommand::make_determinate::{
            ORIGINAL_INSTALLER_BINARY_LOCATION, ORIGINAL_RECEIPT_LOCATION,
        },
    },
    error::HasExpectedErrors,
    plan::{current_version, BINARY_LOCATION, RECEIPT_LOCATION},
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
            if current_exe.as_path().starts_with("/nix") {
                tracing::debug!(
                    "Detected uninstall from within `/nix`, copying executable and re-executing"
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
            success = "Uninstallation completed successfully!".green().bold(),
        );

        // NOTE(cole-h): If `/nix/original-receipt.json` exists, the current binary was used to
        // install determinate to an existing installation. If they're uninstalling, they must first
        // uninstall determinate (using /nix/nix-installer and /nix/receipt.json, which will be the
        // determinate-installing nix-installer binary and receipt), and once that succeeds, we move
        // the original binary (that installed Nix itself) and its receipt back to their expected
        // locations (`/nix/nix-installer`, `/nix/receipt.json`) and rerun the uninstall command
        // with the same arguments but with the original binary.
        //
        // This is to provide support for installing determinate to installations whose receipts
        // cannot be parsed -- we want to be able to uninstall determinate and Nix, but this is
        // complicated when mixing versions. So we do a best-effort attempt of "keep original binary
        // and receipt under a new name, and put the new determinate-installed binary in the normal
        // place".
        let original_binary_exists = PathBuf::from(ORIGINAL_INSTALLER_BINARY_LOCATION).exists();
        let original_receipt_exists = PathBuf::from(ORIGINAL_RECEIPT_LOCATION).exists();

        if original_receipt_exists && !original_binary_exists {
            #[derive(serde::Deserialize)]
            struct VersionedInstallPlan {
                version: semver::Version,
            }

            let original_plan = tokio::fs::read_to_string(ORIGINAL_RECEIPT_LOCATION)
                .await
                .wrap_err("Reading original receipt")?;
            let versioned_plan: VersionedInstallPlan = serde_json::from_str(&original_plan)
                .wrap_err("Getting version out of original plan")?;

            // FIXME: better message
            tracing::error!("the original nix-installer binary appears to have gone missing, so uninstall cannot proceed; redownload it with the following:");
            tracing::error!("curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix/tag/v{} | sh -s -- uninstall", versioned_plan.version);
            return Err(eyre::eyre!("foobar"));
        }

        if original_receipt_exists {
            tokio::fs::remove_file(BINARY_LOCATION)
                .await
                .wrap_err_with(|| {
                    format!("Removing determinate nix-installer binary at {BINARY_LOCATION}")
                })?;
            tokio::fs::remove_file(RECEIPT_LOCATION)
                .await
                .wrap_err_with(|| format!("Removing determinate receipt at {RECEIPT_LOCATION}"))?;

            tokio::fs::rename(ORIGINAL_INSTALLER_BINARY_LOCATION, BINARY_LOCATION)
                .await
                .wrap_err_with(|| {
                    format!("Moving original nix-installer binary back to {BINARY_LOCATION}")
                })?;
            tokio::fs::rename(ORIGINAL_RECEIPT_LOCATION, RECEIPT_LOCATION)
                .await
                .wrap_err_with(|| format!("Moving original receipt back to {RECEIPT_LOCATION}"))?;

            let args = std::env::args();
            let mut arg_vec_cstring = vec![];
            for arg in args {
                arg_vec_cstring.push(CString::new(arg).wrap_err("Making arg into C string")?);
            }
            let exe_cstring = CString::new("/nix/nix-installer")
                .wrap_err("Making C string of original nix-installer executable path")?;

            tracing::trace!("Execv'ing `{exe_cstring:?} {arg_vec_cstring:?}`");
            nix::unistd::execv(&exe_cstring, &arg_vec_cstring)
                .wrap_err("Executing original `nix-installer`")?;
        }

        Ok(ExitCode::SUCCESS)
    }
}
