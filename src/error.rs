#[derive(thiserror::Error, Debug)]
pub enum HarmonicError {
    #[error("Downloading Nix: {0}")]
    DownloadingNix(#[from] reqwest::Error),
    #[error("Unpacking Nix: {0}")]
    UnpackingNix(#[from] std::io::Error),
}
