mod error;

use error::HarmonicError;
use futures::stream::TryStreamExt;
use reqwest::Url;

// This uses a Rust builder pattern
#[derive(Debug)]
pub struct Harmonic {
    daemon_user_count: usize,
    channels: Vec<Url>,
    modify_profile: bool,
}

impl Harmonic {
    pub fn daemon_user_count(&mut self, count: usize) -> &mut Self {
        self.daemon_user_count = count;
        self
    }

    pub fn channels(&mut self, channels: impl IntoIterator<Item = Url>) -> &mut Self {
        self.channels = channels.into_iter().collect();
        self
    }

    pub fn modify_profile(&mut self, toggle: bool) -> &mut Self {
        self.modify_profile = toggle;
        self
    }
}

#[cfg(target_os = "linux")]
impl Harmonic {
    #[tracing::instrument(skip_all, fields(
        channels = %self.channels.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "),
        daemon_user_count = %self.daemon_user_count,
        modify_profile = %self.modify_profile
    ))]
    pub async fn install(&self) -> Result<(), HarmonicError> {
        self.download_nix().await?;
        Ok(())
    }

    pub async fn download_nix(&self) -> Result<(), HarmonicError> {
        // TODO(@hoverbear): architecture specific download
        // TODO(@hoverbear): hash check
        let res = reqwest::get(
            "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-x86_64-linux.tar.xz",
        )
        .await
        .map_err(HarmonicError::DownloadingNix)?;
        let stream = res.bytes_stream();
        let async_read = stream
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            .into_async_read();
        let buffered = futures::io::BufReader::new(async_read);
        let decoder = async_compression::futures::bufread::XzDecoder::new(buffered);
        let archive = async_tar::Archive::new(decoder);
        archive
            .unpack("boop")
            .await
            .map_err(HarmonicError::UnpackingNix)?;
        tracing::info!("Jobs done!!!");
        Ok(())
    }
}

#[cfg(target_os = "macos")]
impl Harmonic {
    #[tracing::instrument]
    pub async fn install(&self) -> Result<(), HarmonicError> {
        // TODO(@hoverbear): Check MacOS version
        todo!();
        Ok(())
    }

    pub async fn download_nix(&self) -> Result<(), HarmonicError> {
        Ok(())
    }
}

impl Default for Harmonic {
    fn default() -> Self {
        Self {
            channels: vec!["https://nixos.org/channels/nixpkgs-unstable"
                .parse::<Url>()
                .unwrap()],
            daemon_user_count: 32,
            modify_profile: true,
        }
    }
}
