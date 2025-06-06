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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BackendType {
    NixEnv,
    NixProfile,
}

pub async fn get_profile_backend_type(profile: &std::path::Path) -> Option<BackendType> {
    // If the file has a manifest.json, that means `nix profile` touched it, and ONLY `nix profile` can touch it.
    if tokio::fs::metadata(profile.join("manifest.json"))
        .await
        .is_ok()
    {
        return Some(BackendType::NixProfile);
    }

    // If the file has a manifest.nix, that means it was created by `nix-env`.
    if tokio::fs::metadata(profile.join("manifest.nix"))
        .await
        .is_ok()
    {
        return Some(BackendType::NixEnv);
    }

    // If neither of those exist, it can be managed by either, so express no preference.
    return None;
}
