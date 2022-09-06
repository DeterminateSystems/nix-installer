use std::{path::PathBuf, process::ExitCode};

use crate::{cli::CommandExecute, interaction};
use harmonic::NixOs;
use owo_colors::OwoColorize;

/// Install an opinionated NixOS on a device
#[derive(Debug, clap::Parser)]
pub(crate) struct NixOsCommand {
    /// The disk to install on (eg. `/dev/nvme1n1`, `/dev/sda`)
    #[clap(long)]
    target_device: PathBuf,
}

#[async_trait::async_trait]
impl CommandExecute for NixOsCommand {
    #[tracing::instrument(skip_all, fields(
        target_device = %self.target_device.display(),
    ))]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let Self { target_device } = self;

        if interaction::confirm(format!(
            "\
This will:
1. Irrecoverably wipe `{target_device}`
2. Write a GPT partition table to `{target_device}`
3. Write a partition 1 to `{target_device}` as a 1G FAT32 EFI ESP
4. Write partition 2 to `{target_device}` as a BTRFS disk consuming
   the remaining disk\n\
5. Create several BTRFS subvolumes supporting an ephemeral
   (aka \'Erase your darlings\') installation\n\
        ",
            target_device = target_device.display().cyan()
        ))
        .await?
        {
            NixOs::new(target_device).install().await?;
        }

        Ok(ExitCode::SUCCESS)
    }
}
