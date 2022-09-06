use std::path::PathBuf;

use crate::HarmonicError;

pub struct NixOs {
    /// The disk to install on (eg. `/dev/nvme1n1`, `/dev/sda`)
    target_device: PathBuf,
}

impl NixOs {
    pub fn new(target_device: PathBuf) -> Self {
        Self { target_device }
    }
    #[tracing::instrument(skip_all, fields(
        target_device = %self.target_device.display(),
    ))]
    pub async fn install(&self) -> Result<(), HarmonicError> {
        tracing::warn!("Kicking your socks in");
        Ok(())
    }
}
