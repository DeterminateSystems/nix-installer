use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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
    StartNixCommand(&'static str, std::io::Error),

    #[error("Failed to run the nix command `{0}`: {1:?}")]
    NixCommand(&'static str, std::process::Output),
    #[error("Failed to add the package {0} to the profile: {1:?}")]
    AddPackage(PathBuf, std::process::Output),

    #[error("Failed to update the user's profile at {0}: {1:?}")]
    UpdateProfile(PathBuf, std::process::Output),

    #[error("Deserializing installed packages: {0}")]
    Deserialization(#[from] serde_json::Error),

    #[cfg(test)]
    #[error("Failed to create a temp file named `{0}`: {1}")]
    CreateTempFile(PathBuf, std::io::Error),

    #[cfg(test)]
    #[error("Failed to write to a file named `{0}`: {1}")]
    Write(PathBuf, std::io::Error),

    #[cfg(test)]
    #[error("Failed to add a path to the store {0:?}")]
    AddPathFailed(std::ffi::OsString),
}

pub(crate) struct NixEnv<'a> {
    pub nix_store_path: &'a Path,
    pub nss_ca_cert_path: &'a Path,

    pub profile: &'a Path,
    pub pkgs: &'a [&'a Path],
}

impl NixEnv<'_> {
    pub(crate) async fn install_packages(&self) -> Result<(), NixEnvError> {
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

        let tmp = tempfile::tempdir().map_err(NixEnvError::CreateTempDir)?;
        let temporary_profile = tmp.path().join("profile");

        // Construct an empty profile
        {
            // See: https://github.com/DeterminateSystems/nix-src/blob/f60b21563990ec11d87dd4abe57b8b187d6b6fb3/src/nix-env/buildenv.nix
            let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix"))
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
                .arg(&temporary_profile)
                .output()
                .await
                .map_err(|e| NixEnvError::StartNixCommand("nix build-ing an empty profile", e))?;

            if !output.status.success() {
                return Err(NixEnvError::NixCommand(
                    "nix build-ing an empty profile",
                    output,
                ));
            }
        }

        if let Ok(canon_profile) = self.profile.canonicalize() {
            tracing::info!("Duplicating the existing profile into the scratch profile");

            let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
                .set_nix_options(self.nss_ca_cert_path)?
                .arg("--profile")
                .arg(&temporary_profile)
                .arg("--set")
                .arg(canon_profile)
                .output()
                .await
                .map_err(|e| {
                    NixEnvError::StartNixCommand(
                        "Duplicating the default profile into the scratch profile",
                        e,
                    )
                })?;

            if !output.status.success() {
                return Err(NixEnvError::NixCommand(
                    "Duplicating the default profile into the scratch profile",
                    output,
                ));
            }
        }

        // Query packages that are already installed in the profile.
        // Constructs a map of (store path in the profile) -> (hash set of paths that are inside that store path)
        let mut installed_paths: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
        {
            let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
                .set_nix_options(self.nss_ca_cert_path)?
                .arg("--profile")
                .arg(&temporary_profile)
                .args(["--query", "--installed", "--out-path", "--json"])
                .stdin(std::process::Stdio::null())
                .output()
                .await
                .map_err(|e| {
                    NixEnvError::StartNixCommand("nix-env --query'ing installed packages", e)
                })?;

            if !output.status.success() {
                return Err(NixEnvError::NixCommand(
                    "nix-env --query'ing installed packages",
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

        for pkg in self.pkgs {
            let pkg_outputs =
                collect_children(pkg).map_err(NixEnvError::EnumeratingStorePathContent)?;

            for (root_path, children) in &installed_paths {
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

                    {
                        let output =
                            tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
                                .set_nix_options(self.nss_ca_cert_path)?
                                .arg("--profile")
                                .arg(&temporary_profile)
                                .arg("--uninstall")
                                .arg(root_path)
                                .output()
                                .await
                                .map_err(|e| {
                                    NixEnvError::StartNixCommand(
                                        "nix-env --uninstall'ing a conflicting package",
                                        e,
                                    )
                                })?;

                        if !output.status.success() {
                            return Err(NixEnvError::NixCommand(
                                "nix-env --uninstall'ing a conflicting package",
                                output,
                            ));
                        }
                    }
                }
            }

            let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
                .set_nix_options(self.nss_ca_cert_path)
                .arg("--profile")
                .arg(&temporary_profile)
                .arg("--install")
                .arg(pkg)
                .output()
                .await
                .map_err(|e| {
                    NixEnvError::StartNixCommand("Adding a new package to the profile", e)
                })?;

            if !output.status.success() {
                return Err(NixEnvError::AddPackage(pkg.to_path_buf(), output));
            }
        }

        // Finish by setting the user provided profile to the new version we've constructed
        {
            let output = tokio::process::Command::new(self.nix_store_path.join("bin/nix-env"))
                .set_nix_options(self.nss_ca_cert_path)?
                .arg("--profile")
                .arg(self.profile)
                .arg("--set")
                .arg(&temporary_profile)
                .output()
                .await
                .map_err(|e| {
                    NixEnvError::StartNixCommand(
                        "nix-env --profile ... --set ... the user's profile",
                        e,
                    )
                })?;

            if !output.status.success() {
                return Err(NixEnvError::UpdateProfile(
                    self.profile.to_path_buf(),
                    output,
                ));
            }
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
    let paths = walkdir::WalkDir::new(&base_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().is_dir() {
                None
            } else {
                Some(entry)
            }
        })
        .map(|entry| {
            entry
                .path()
                .strip_prefix(&base_path)
                .unwrap_or_else(|_| entry.path())
                .to_path_buf()
        })
        .collect::<HashSet<PathBuf>>();
    Ok(paths)
}

trait Nixy {
    fn set_nix_options(
        &mut self,
        nss_ca_cert_pkg: &Path,
    ) -> Result<&mut tokio::process::Command, NixEnvError>;
}

impl Nixy for tokio::process::Command {
    fn set_nix_options(
        &mut self,
        nss_ca_cert_pkg: &Path,
    ) -> Result<&mut tokio::process::Command, NixEnvError> {
        Ok(self
            .process_group(0)
            .args(["--option", "substitute", "false"])
            .args(["--option", "post-build-hook", ""])
            .env("HOME", dirs::home_dir().ok_or(NixEnvError::NoRootHome)?)
            .env(
                "NIX_SSL_CERT_FILE",
                nss_ca_cert_pkg.join("etc/ssl/certs/ca-bundle.crt"),
            ))
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;
    use std::path::{Path, PathBuf};

    use tokio::io::AsyncWriteExt;

    use super::NixEnv;
    use super::NixEnvError;
    use super::Nixy;

    async fn should_skip() -> bool {
        let cmdret = tokio::process::Command::new("nix")
            .set_nix_options(Path::new("/dev/null"))
            .unwrap()
            .arg("--version")
            .output()
            .await;

        if cmdret.is_ok() {
            return false;
        } else {
            println!("Skipping this test because nix isn't in PATH");
            return true;
        }
    }

    async fn sample_tree(
        dirname: &str,
        filename: &str,
        content: &str,
    ) -> Result<PathBuf, NixEnvError> {
        let temp_dir = tempfile::tempdir().map_err(NixEnvError::CreateTempDir)?;

        let sub_dir = temp_dir.path().join(dirname);
        tokio::fs::create_dir(&sub_dir)
            .await
            .map_err(NixEnvError::CreateTempDir)?;

        let file = sub_dir.join(filename);

        let mut f = tokio::fs::File::options()
            .create(true)
            .write(true)
            .open(&file)
            .await
            .map_err(|e| NixEnvError::CreateTempFile(file.to_path_buf(), e))?;

        f.write_all(content.as_bytes())
            .await
            .map_err(|e| NixEnvError::Write(file.to_path_buf(), e))?;

        let mut cmdret = tokio::process::Command::new("nix")
            .set_nix_options(Path::new("/dev/null"))
            .unwrap()
            .args(&["store", "add"])
            .arg(&sub_dir)
            .output()
            .await
            .map_err(|e| NixEnvError::StartNixCommand("nix store add", e))?;

        if !cmdret.status.success() {
            return Err(NixEnvError::NixCommand("nix store add", cmdret));
        }

        if cmdret.stdout.last() == Some(&b'\n') {
            cmdret.stdout.remove(cmdret.stdout.len() - 1);
        }

        let p = PathBuf::from(std::ffi::OsString::from_vec(cmdret.stdout));

        if p.exists() {
            Ok(p)
        } else {
            Err(NixEnvError::AddPathFailed(std::ffi::OsString::from_vec(
                cmdret.stderr,
            )))
        }
    }

    #[tokio::test]
    async fn test_detect_intersection() {
        if should_skip().await {
            return;
        }

        let profile = tempfile::tempdir().unwrap();
        let profile_path = profile.path().join("profile");

        let tree_1 = sample_tree("foo", "foo", "a").await.unwrap();
        let tree_2 = sample_tree("bar", "foo", "b").await.unwrap();

        (NixEnv {
            nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
            nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
            profile: &profile_path,
            pkgs: &[&tree_1, &tree_2],
        })
        .install_packages()
        .await
        .unwrap_err();
    }

    #[tokio::test]
    async fn test_no_intersection() {
        if should_skip().await {
            return;
        }

        let profile = tempfile::tempdir().unwrap();
        let profile_path = profile.path().join("profile");

        let tree_1 = sample_tree("foo", "foo", "a").await.unwrap();
        let tree_2 = sample_tree("bar", "bar", "b").await.unwrap();

        (NixEnv {
            nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
            nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
            profile: &profile_path,
            pkgs: &[&tree_1, &tree_2],
        })
        .install_packages()
        .await
        .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(profile_path.join("foo"))
                .await
                .unwrap(),
            "a"
        );
        assert_eq!(
            tokio::fs::read_to_string(profile_path.join("bar"))
                .await
                .unwrap(),
            "b"
        );

        let tree_3 = sample_tree("baz", "baz", "c").await.unwrap();
        let tree_4 = sample_tree("tux", "tux", "d").await.unwrap();

        (NixEnv {
            nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
            nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
            profile: &profile_path,
            pkgs: &[&tree_3, &tree_4],
        })
        .install_packages()
        .await
        .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(profile_path.join("baz"))
                .await
                .unwrap(),
            "c"
        );
        assert_eq!(
            tokio::fs::read_to_string(profile_path.join("tux"))
                .await
                .unwrap(),
            "d"
        );
    }

    #[tokio::test]
    async fn test_overlap_replaces() {
        if should_skip().await {
            return;
        }

        let profile = tempfile::tempdir().unwrap();
        let profile_path = profile.path().join("profile");

        let tree_base = sample_tree("fizz", "fizz", "fizz").await.unwrap();
        let tree_1 = sample_tree("foo", "foo", "a").await.unwrap();
        (NixEnv {
            nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
            nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
            profile: &profile_path,
            pkgs: &[&tree_base, &tree_1],
        })
        .install_packages()
        .await
        .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(profile_path.join("fizz"))
                .await
                .unwrap(),
            "fizz"
        );
        assert_eq!(
            tokio::fs::read_to_string(profile_path.join("foo"))
                .await
                .unwrap(),
            "a"
        );

        let tree_2 = sample_tree("foo", "foo", "b").await.unwrap();
        (NixEnv {
            nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
            nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
            profile: &profile_path,
            pkgs: &[&tree_2],
        })
        .install_packages()
        .await
        .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(profile_path.join("foo"))
                .await
                .unwrap(),
            "b"
        );

        let tree_3 = sample_tree("bar", "foo", "c").await.unwrap();
        (NixEnv {
            nix_store_path: Path::new("/nix/var/nix/profiles/default/"),
            nss_ca_cert_path: Path::new("/nix/var/nix/profiles/default/"),
            profile: &profile_path,
            pkgs: &[&tree_3],
        })
        .install_packages()
        .await
        .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(profile_path.join("foo"))
                .await
                .unwrap(),
            "c"
        );

        assert_eq!(
            tokio::fs::read_to_string(profile_path.join("fizz"))
                .await
                .unwrap(),
            "fizz"
        );
    }
}
