use std::path::PathBuf;

pub(crate) mod nixenv;
pub(crate) mod nixprofile;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not identify a home directory for root")]
    NoRootHome,

    #[error("Failed to enumerate a store path: {0}")]
    EnumeratingStorePathContent(std::io::Error),

    #[error("The following package has paths that intersect with other paths in other packages you want to install: {0}. Paths: {1:?}")]
    PathConflict(PathBuf, Vec<PathBuf>),

    #[error("Failed to create a temp dir: {0}")]
    CreateTempDir(std::io::Error),

    #[error("Failed to start the nix command `{0}`: {1}")]
    StartNixCommand(String, std::io::Error),

    #[error("Failed to run the nix command `{0}`: {1:?}")]
    NixCommand(String, std::process::Output),
    #[error("Failed to add the package {0} to the profile: {1:?}")]
    AddPackage(PathBuf, std::process::Output),

    #[error("Failed to update the user's profile at {0}: {1:?}")]
    UpdateProfile(PathBuf, std::process::Output),

    #[error("Deserializing the list of installed packages for the profile: {0}")]
    Deserialization(#[from] serde_json::Error),
}

pub enum WriteToDefaultProfile {
    WriteToDefault,

    #[cfg(test)]
    Isolated,
}
