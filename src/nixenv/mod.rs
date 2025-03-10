use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum NixEnvError {
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

pub(crate) struct NixEnv<'a> {
    pub nix_store_path: &'a Path,
    pub nss_ca_cert_path: &'a Path,

    pub profile: &'a Path,
    pub pkgs: &'a [&'a Path],
}

pub enum WriteToDefaultProfile {
    WriteToDefault,

    #[cfg(test)]
    Isolated,
}

impl NixEnv<'_> {
    pub(crate) async fn install_packages(
        &self,
        to_default: WriteToDefaultProfile,
    ) -> Result<(), NixEnvError> {
        self.validate_paths_can_cohabitate().await?;

        let tmp = tempfile::tempdir().map_err(NixEnvError::CreateTempDir)?;
        let temporary_profile = tmp.path().join("profile");

        self.make_empty_profile(&temporary_profile).await?;

        if let Ok(canon_profile) = self.profile.canonicalize() {
            self.set_profile_to(Some(&temporary_profile), &canon_profile)
                .await?;
        }

        let paths_by_pkg_output = self
            .collect_paths_by_package_output(&temporary_profile)
            .await?;

        for pkg in self.pkgs {
            let pkg_outputs =
                collect_children(pkg).map_err(NixEnvError::EnumeratingStorePathContent)?;

            for (root_path, children) in &paths_by_pkg_output {
                let conflicts = children
                    .intersection(&pkg_outputs)
                    .collect::<Vec<&PathBuf>>();

                if !conflicts.is_empty() {
                    tracing::debug!(
                        ?temporary_profile,
                        ?root_path,
                        ?conflicts,
                        "Uninstalling path from the scratch profile due to conflicts"
                    );

                    self.uninstall_path(&temporary_profile, root_path).await?;
                }
            }

            self.install_path(&temporary_profile, pkg).await?;
        }

        self.set_profile_to(
            match to_default {
                #[cfg(test)]
                WriteToDefaultProfile::Isolated => Some(self.profile),
                WriteToDefaultProfile::WriteToDefault => None,
            },
            &temporary_profile,
        )
        .await?;

        Ok(())
    }

    /// Collect all the paths in the new set of packages.
    /// Returns an error if they have paths that will conflict with each other when installed.
    async fn validate_paths_can_cohabitate(&self) -> Result<HashSet<PathBuf>, NixEnvError> {
        let mut all_new_paths = HashSet::<PathBuf>::new();

        for pkg in self.pkgs {
            let candidates =
                collect_children(pkg).map_err(NixEnvError::EnumeratingStorePathContent)?;

            let intersection = candidates
                .intersection(&all_new_paths)
                .cloned()
                .collect::<Vec<PathBuf>>();
            if !intersection.is_empty() {
                return Err(NixEnvError::PathConflict(pkg.to_path_buf(), intersection));
            }

            all_new_paths.extend(candidates.into_iter());
        }

        Ok(all_new_paths)
    }

    async fn make_empty_profile(&self, profile: &Path) -> Result<(), NixEnvError> {
        // See: https://github.com/DeterminateSystems/nix-src/blob/f60b21563990ec11d87dd4abe57b8b187d6b6fb3/src/nix-env/buildenv.nix
        let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix"))
            .process_group(0)
            .set_nix_options(self.nss_ca_cert_path)?
            .args([
                "build",
                "--expr",
                r#"
                    derivation {
                        name = "user-environment";
                        system = "builtin";
                        builder = "builtin:buildenv";
                        derivations = [];
                        manifest = builtins.toFile "env-manifest.nix" "[]";
                    }
                "#,
                "--out-link",
            ])
            .arg(profile)
            .output()
            .await
            .map_err(|e| {
                NixEnvError::StartNixCommand("nix build-ing an empty profile".to_string(), e)
            })?;

        if !output.status.success() {
            return Err(NixEnvError::NixCommand(
                "nix build-ing an empty profile".to_string(),
                output,
            ));
        }

        Ok(())
    }

    async fn set_profile_to(
        &self,
        profile: Option<&Path>,
        canon_profile: &Path,
    ) -> Result<(), NixEnvError> {
        tracing::debug!("Duplicating the existing profile into the scratch profile");

        let mut cmd = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"));

        cmd.process_group(0);
        cmd.set_nix_options(self.nss_ca_cert_path)?;

        if let Some(profile) = profile {
            cmd.arg("--profile");
            cmd.arg(profile);
        }

        let output = cmd
            .arg("--set")
            .arg(canon_profile)
            .output()
            .await
            .map_err(|e| {
                NixEnvError::StartNixCommand(
                    "Duplicating the default profile into the scratch profile".to_string(),
                    e,
                )
            })?;

        if !output.status.success() {
            return Err(NixEnvError::NixCommand(
                "Duplicating the default profile into the scratch profile".to_string(),
                output,
            ));
        }

        Ok(())
    }

    async fn collect_paths_by_package_output(
        &self,
        profile: &Path,
    ) -> Result<HashMap<PathBuf, HashSet<PathBuf>>, NixEnvError> {
        // Query packages that are already installed in the profile.
        // Constructs a map of (store path in the profile) -> (hash set of paths that are inside that store path)
        let mut installed_paths: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
        {
            let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
                .process_group(0)
                .set_nix_options(self.nss_ca_cert_path)?
                .arg("--profile")
                .arg(profile)
                .args(["--query", "--installed", "--out-path", "--json"])
                .stdin(std::process::Stdio::null())
                .output()
                .await
                .map_err(|e| {
                    NixEnvError::StartNixCommand(
                        "nix-env --query'ing installed packages".to_string(),
                        e,
                    )
                })?;

            if !output.status.success() {
                return Err(NixEnvError::NixCommand(
                    "nix-env --query'ing installed packages".to_string(),
                    output,
                ));
            }

            let installed_pkgs: HashMap<String, PackageInfo> =
                serde_json::from_slice(&output.stdout)?;
            for pkg in installed_pkgs.values() {
                for path in pkg.outputs.values() {
                    installed_paths
                        .insert(path.clone(), collect_children(path).unwrap_or_default());
                }
            }
        }

        Ok(installed_paths)
    }

    async fn uninstall_path(&self, profile: &Path, remove: &Path) -> Result<(), NixEnvError> {
        let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
            .process_group(0)
            .set_nix_options(self.nss_ca_cert_path)?
            .arg("--profile")
            .arg(profile)
            .arg("--uninstall")
            .arg(remove)
            .output()
            .await
            .map_err(|e| {
                NixEnvError::StartNixCommand(
                    format!("nix-env --uninstall'ing conflicting package {:?}", remove),
                    e,
                )
            })?;

        if !output.status.success() {
            return Err(NixEnvError::NixCommand(
                format!("nix-env --uninstall'ing conflicting package {:?}", remove),
                output,
            ));
        }

        Ok(())
    }

    async fn install_path(&self, profile: &Path, add: &Path) -> Result<(), NixEnvError> {
        let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
            .process_group(0)
            .set_nix_options(self.nss_ca_cert_path)?
            .arg("--profile")
            .arg(profile)
            .arg("--install")
            .arg(add)
            .output()
            .await
            .map_err(|e| {
                NixEnvError::StartNixCommand(
                    format!("Adding the package {:?} to the profile", add),
                    e,
                )
            })?;

        if !output.status.success() {
            return Err(NixEnvError::AddPackage(add.to_path_buf(), output));
        }

        Ok(())
    }
}

#[derive(Debug, serde::Deserialize)]
struct PackageInfo {
    #[serde(default)]
    outputs: HashMap<String, PathBuf>,
}

fn collect_children<P: AsRef<std::path::Path>>(
    base_path: P,
) -> Result<HashSet<PathBuf>, std::io::Error> {
    let base_path = base_path.as_ref();
    let paths = walkdir::WalkDir::new(base_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| -> Option<walkdir::DirEntry> {
            let entry = entry
                .inspect_err(
                    |e| tracing::debug!(?base_path, %e, "Error walking the file tree, skipping."),
                )
                .ok()?;

            if entry.file_type().is_dir() {
                None
            } else {
                Some(entry)
            }
        })
        .filter_map(|entry| {
            entry.path()
                .strip_prefix(base_path)
                .inspect_err(
                    |e| tracing::debug!(?base_path, path = ?entry.path(), %e, "Error stripping the prefix from the path, skipping."),
                )
                .ok()
                .map(PathBuf::from)
        })
        .collect::<HashSet<PathBuf>>();
    Ok(paths)
}

trait NixCommandExt {
    fn set_nix_options(
        &mut self,
        nss_ca_cert_pkg: &Path,
    ) -> Result<&mut tokio::process::Command, NixEnvError>;
}

impl NixCommandExt for tokio::process::Command {
    fn set_nix_options(
        &mut self,
        nss_ca_cert_pkg: &Path,
    ) -> Result<&mut tokio::process::Command, NixEnvError> {
        Ok(self
            .args(["--option", "substitute", "false"])
            .args(["--option", "post-build-hook", ""])
            .env("HOME", dirs::home_dir().ok_or(NixEnvError::NoRootHome)?)
            .env(
                "NIX_SSL_CERT_FILE",
                nss_ca_cert_pkg.join("etc/ssl/certs/ca-bundle.crt"),
            ))
    }
}
