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
    #[error("Opening file `{0}`")]
    OpeningFile(std::path::PathBuf, std::io::Error),
    #[error("Writing to file `{0}`")]
    WritingFile(std::path::PathBuf, std::io::Error),
    #[error("Getting tempdir")]
    GettingTempDir(std::io::Error),
    #[error("Installing fetched Nix into the new store")]
    InstallNixIntoStore(std::io::Error),
    #[error("Installing fetched nss-cacert into the new store")]
    InstallNssCacertIntoStore(std::io::Error),
    #[error("Updating the Nix channel")]
    UpdatingNixChannel(std::io::Error),
    #[error("Globbing pattern error")]
    GlobPatternError(glob::PatternError),
    #[error("Could not find nss-cacert")]
    NoNssCacert,
    #[error("Creating /etc/nix/nix.conf")]
    CreatingNixConf(std::io::Error),
    #[error("No supported init syustem found")]
    InitNotSupported,
    #[error("Linking `{0}` to `{1}`")]
    Linking(std::path::PathBuf, std::path::PathBuf, std::io::Error),
    #[error("Running `systemd-tmpfiles`")]
    SystemdTmpfiles(std::io::Error),
    #[error("Command `{0}` failed to execute")]
    CommandFailedExec(String, std::io::Error),
    // TODO(@Hoverbear): This should capture the stdout.
    #[error("Command `{0}` did not to return a success status")]
    CommandFailedStatus(String),
    #[error("Join error")]
    JoinError(#[from] tokio::task::JoinError),
}
