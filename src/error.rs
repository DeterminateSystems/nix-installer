#[derive(thiserror::Error, Debug)]
pub enum HarmonicError {
    #[error("Request error")]
    Reqwest(#[from] reqwest::Error),
    #[error("Unarchiving error")]
    Unarchive(std::io::Error),
    #[error("Getting temporary directory")]
    TempDir(std::io::Error),
    #[error("Glob pattern error")]
    GlobPatternError(#[from] glob::PatternError),
    #[error("Glob globbing error")]
    GlobGlobError(#[from] glob::GlobError),
    #[error("Symlinking from `{0}` to `{1}`")]
    Symlink(std::path::PathBuf, std::path::PathBuf, std::io::Error),
    #[error("Renaming from `{0}` to `{1}`")]
    Rename(std::path::PathBuf, std::path::PathBuf, std::io::Error),
    #[error("Unarchived Nix store did not appear to include a `nss-cacert` location")]
    NoNssCacert,
    #[error("No supported init system found")]
    InitNotSupported,
    #[error("Creating directory `{0}`")]
    CreateDirectory(std::path::PathBuf, std::io::Error),
    #[error("Walking directory `{0}`")]
    WalkDirectory(std::path::PathBuf, walkdir::Error),
    #[error("Setting permissions `{0}`")]
    SetPermissions(std::path::PathBuf, std::io::Error),
    #[error("Command `{0}` failed to execute")]
    CommandFailedExec(String, std::io::Error),
    // TODO(@Hoverbear): This should capture the stdout.
    #[error("Command `{0}` did not to return a success status")]
    CommandFailedStatus(String),
    #[error("Join error")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("Opening file `{0}` for writing")]
    OpenFile(std::path::PathBuf, std::io::Error),
    #[error("Opening file `{0}` for writing")]
    WriteFile(std::path::PathBuf, std::io::Error),
    #[error("Seeking file `{0}` for writing")]
    SeekFile(std::path::PathBuf, std::io::Error),
    #[error("Changing ownership of `{0}`")]
    Chown(std::path::PathBuf, nix::errno::Errno),
    #[error("Getting uid for user `{0}`")]
    UserId(String, nix::errno::Errno),
    #[error("Getting user `{0}`")]
    NoUser(String),
    #[error("Getting gid for group `{0}`")]
    GroupId(String, nix::errno::Errno),
    #[error("Getting group `{0}`")]
    NoGroup(String),
}
