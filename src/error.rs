#[derive(thiserror::Error, Debug)]
pub enum HarmonicError {
    #[error("Downloading Nix")]
    DownloadingNix(#[from] reqwest::Error),
    #[error("Unpacking Nix")]
    UnpackingNix(std::io::Error),
    #[error("Running `groupadd`")]
    GroupAddSpawn(std::io::Error),
    #[error("`groupadd` returned failure")]
    GroupAddFailure(std::process::ExitStatus),
    #[error("Running `useradd`")]
    UserAddSpawn(std::io::Error),
    #[error("`useradd` returned failure")]
    UserAddFailure(std::process::ExitStatus),
    #[error("Creating directory")]
    CreateDirectory(std::io::Error),
    #[error("Placing channel configuration")]
    PlaceChannelConfiguration(std::io::Error),
}
