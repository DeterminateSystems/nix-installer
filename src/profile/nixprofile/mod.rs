use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

pub(crate) struct NixProfile<'a> {
    pub nix_store_path: &'a Path,
    pub nss_ca_cert_path: &'a Path,

    pub profile: &'a Path,
    pub pkgs: &'a [&'a Path],
}

impl NixProfile<'_> {
    pub(crate) async fn install_packages(
        &self,
        to_default: super::WriteToDefaultProfile,
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

            for (element, children) in &paths_by_pkg_output {
                let conflicts = children
                    .intersection(&pkg_outputs)
                    .collect::<Vec<&PathBuf>>();

                if !conflicts.is_empty() {
                    tracing::debug!(
                        ?temporary_profile,
                        ?element,
                        ?conflicts,
                        "Uninstalling element from the scratch profile due to conflicts"
                    );

                    self.uninstall_element(&temporary_profile, element).await?;
                }
            }

            self.install_path(&temporary_profile, pkg).await?;
        }

        self.set_profile_to(
            match to_default {
                #[cfg(test)]
                super::WriteToDefaultProfile::Isolated => Some(self.profile),
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
    ) -> Result<HashMap<String, HashSet<PathBuf>>, super::Error> {
        // Query packages that are already installed in the profile.
        // Constructs a map of (store path in the profile) -> (hash set of paths that are inside that store path)
        let mut installed_paths: HashMap<String, HashSet<PathBuf>> = HashMap::new();
        {
            let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix"))
                .process_group(0)
                .set_nix_options(self.nss_ca_cert_path)?
                .arg("profile")
                .arg("list")
                .arg("--profile")
                .arg(profile)
                .arg("--json")
                .stdin(std::process::Stdio::null())
                .output()
                .await
                .map_err(|e| {
                    super::Error::StartNixCommand(
                        "nix profile list'ing installed packages".to_string(),
                        e,
                    )
                })?;

            if !output.status.success() {
                return Err(super::Error::NixCommand(
                    "nix profile list'ing installed packages".to_string(),
                    output,
                ));
            }

            #[derive(serde::Deserialize)]
            struct ProfileList {
                elements: HashMap<String, ProfileElement>,
            }

            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct ProfileElement {
                store_paths: Vec<PathBuf>,
            }

            let installed_pkgs: ProfileList = serde_json::from_slice(&output.stdout)?;
            for (name, element) in installed_pkgs.elements.into_iter() {
                installed_paths.insert(
                    name,
                    element
                        .store_paths
                        .into_iter()
                        .flat_map(|path| collect_children(path).unwrap_or_default())
                        .collect(),
                );
            }
        }

        Ok(installed_paths)
    }

    async fn uninstall_element(&self, profile: &Path, element: &str) -> Result<(), super::Error> {
        let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix"))
            .process_group(0)
            .set_nix_options(self.nss_ca_cert_path)?
            .arg("profile")
            .arg("remove")
            .arg("--profile")
            .arg(profile)
            .arg(element)
            .output()
            .await
            .map_err(|e| {
                super::Error::StartNixCommand(
                    format!("nix profile remove'ing conflicting package {:?}", element),
                    e,
                )
            })?;

        if !output.status.success() {
            return Err(super::Error::NixCommand(
                format!("nix profile remove'ing conflicting package {:?}", element),
                output,
            ));
        }

        Ok(())
    }

    async fn install_path(&self, profile: &Path, add: &Path) -> Result<(), super::Error> {
        let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix"))
            .process_group(0)
            .set_nix_options(self.nss_ca_cert_path)?
            .arg("profile")
            .arg("install") // "add" in determinate nix, but "install" is an alias
            .arg("--profile")
            .arg(profile)
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
    #[allow(dead_code)]
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
    ) -> Result<&mut tokio::process::Command, super::Error>;
}

impl NixCommandExt for tokio::process::Command {
    fn set_nix_options(
        &mut self,
        nss_ca_cert_pkg: &Path,
    ) -> Result<&mut tokio::process::Command, super::Error> {
        Ok(self
            .args(["--option", "substitute", "false"])
            .args(["--option", "post-build-hook", ""])
            .env("HOME", dirs::home_dir().ok_or(super::Error::NoRootHome)?)
            .env(
                "NIX_SSL_CERT_FILE",
                nss_ca_cert_pkg.join("etc/ssl/certs/ca-bundle.crt"),
            ))
    }
}
