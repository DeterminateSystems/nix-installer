use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::profile::NixCommandExt;

#[cfg(test)]
mod tests;

pub(crate) struct NixEnv<'a> {
    pub nix_store_path: &'a Path,

    pub profile: &'a Path,
    pub pkgs: &'a [&'a Path],
}

impl NixEnv<'_> {
    pub(crate) async fn remove_conflicts(
        &self,
        to_default: super::WriteToDefaultProfile,
    ) -> Result<(), super::Error> {
        self.install_packages_impl(to_default, false).await
    }

    pub(crate) async fn install_packages(
        &self,
        to_default: super::WriteToDefaultProfile,
    ) -> Result<(), super::Error> {
        self.install_packages_impl(to_default, true).await
    }

    async fn install_packages_impl(
        &self,
        to_default: super::WriteToDefaultProfile,
        install_packages: bool,
    ) -> Result<(), super::Error> {
        self.validate_paths_can_cohabitate().await?;

        let tmp = tempfile::tempdir().map_err(super::Error::CreateTempDir)?;
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
                collect_children(pkg).map_err(super::Error::EnumeratingStorePathContent)?;

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

            if install_packages {
                self.install_path(&temporary_profile, pkg).await?;
            }
        }

        self.set_profile_to(
            match to_default {
                super::WriteToDefaultProfile::Specific => Some(self.profile),
                super::WriteToDefaultProfile::WriteToDefault => None,
            },
            &temporary_profile,
        )
        .await?;

        Ok(())
    }

    /// Collect all the paths in the new set of packages.
    /// Returns an error if they have paths that will conflict with each other when installed.
    async fn validate_paths_can_cohabitate(&self) -> Result<HashSet<PathBuf>, super::Error> {
        let mut all_new_paths = HashSet::<PathBuf>::new();

        for pkg in self.pkgs {
            let candidates =
                collect_children(pkg).map_err(super::Error::EnumeratingStorePathContent)?;

            let intersection = candidates
                .intersection(&all_new_paths)
                .cloned()
                .collect::<Vec<PathBuf>>();
            if !intersection.is_empty() {
                return Err(super::Error::PathConflict(pkg.to_path_buf(), intersection));
            }

            all_new_paths.extend(candidates.into_iter());
        }

        Ok(all_new_paths)
    }

    async fn make_empty_profile(&self, profile: &Path) -> Result<(), super::Error> {
        // See: https://github.com/DeterminateSystems/nix-src/blob/f60b21563990ec11d87dd4abe57b8b187d6b6fb3/src/nix-env/buildenv.nix
        let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix"))
            .process_group(0)
            .set_nix_options()?
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
                super::Error::StartNixCommand("nix build-ing an empty profile".to_string(), e)
            })?;

        if !output.status.success() {
            return Err(super::Error::NixCommand(
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
    ) -> Result<(), super::Error> {
        tracing::debug!("Duplicating the existing profile into the scratch profile");

        let mut cmd = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"));

        cmd.process_group(0);
        cmd.set_nix_options()?;

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
                super::Error::StartNixCommand(
                    "Duplicating the default profile into the scratch profile".to_string(),
                    e,
                )
            })?;

        if !output.status.success() {
            return Err(super::Error::NixCommand(
                "Duplicating the default profile into the scratch profile".to_string(),
                output,
            ));
        }

        Ok(())
    }

    async fn collect_paths_by_package_output(
        &self,
        profile: &Path,
    ) -> Result<HashMap<PathBuf, HashSet<PathBuf>>, super::Error> {
        // Query packages that are already installed in the profile.
        // Constructs a map of (store path in the profile) -> (hash set of paths that are inside that store path)
        let mut installed_paths: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
        {
            let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
                .process_group(0)
                .set_nix_options()?
                .arg("--profile")
                .arg(profile)
                .args(["--query", "--installed", "--out-path", "--json"])
                .stdin(std::process::Stdio::null())
                .output()
                .await
                .map_err(|e| {
                    super::Error::StartNixCommand(
                        "nix-env --query'ing installed packages".to_string(),
                        e,
                    )
                })?;

            if !output.status.success() {
                return Err(super::Error::NixCommand(
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

    async fn uninstall_path(&self, profile: &Path, remove: &Path) -> Result<(), super::Error> {
        let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
            .process_group(0)
            .set_nix_options()?
            .arg("--profile")
            .arg(profile)
            .arg("--uninstall")
            .arg(remove)
            .output()
            .await
            .map_err(|e| {
                super::Error::StartNixCommand(
                    format!("nix-env --uninstall'ing conflicting package {:?}", remove),
                    e,
                )
            })?;

        if !output.status.success() {
            return Err(super::Error::NixCommand(
                format!("nix-env --uninstall'ing conflicting package {:?}", remove),
                output,
            ));
        }

        Ok(())
    }

    async fn install_path(&self, profile: &Path, add: &Path) -> Result<(), super::Error> {
        let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
            .process_group(0)
            .set_nix_options()?
            .arg("--profile")
            .arg(profile)
            .arg("--install")
            .arg(add)
            .output()
            .await
            .map_err(|e| {
                super::Error::StartNixCommand(
                    format!("Adding the package {:?} to the profile", add),
                    e,
                )
            })?;

        if !output.status.success() {
            return Err(super::Error::AddPackage(add.to_path_buf(), output));
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
