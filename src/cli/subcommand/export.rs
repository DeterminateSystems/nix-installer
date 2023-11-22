use std::collections::HashMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io::{stdout, BufWriter, Write};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::cli::CommandExecute;
use clap::Parser;

use tracing::{debug, warn};

const LOCAL_STATE_DIR: &str = "/nix/var";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The HOME environment variable is not set.")]
    HomeNotSet,

    #[error("__ETC_PROFILE_NIX_SOURCED is set, indicating the relevant environment variables have already been set.")]
    AlreadyRun,

    #[error("Some of the paths for XDG_DATA_DIR are not valid, due to an illegal character, like a space or colon.")]
    InvalidXdgDataDirs(Vec<PathBuf>),

    #[error("Some of the paths for PATH are not valid, due to an illegal character, like a space or colon.")]
    InvalidPathDirs(Vec<PathBuf>),

    #[error("Some of the paths for MANPATH are not valid, due to an illegal character, like a space or colon.")]
    InvalidManPathDirs(Vec<PathBuf>),
}

/**
Emit all the environment variables that should be set to use Nix.

Safety note: environment variables and values can contain any bytes except
for a null byte. This includes newlines and spaces, which requires careful
handling.

In `space-newline-separated` mode, `nix-installer` guarantees it will:

  * only emit keys that are alphanumeric with underscores,
  * only emit values without newlines

and will refuse to emit any output to stdout if the variables and values
would violate these safety rules.

In `null-separated` mode, `nix-installer` emits data in this format:

  KEYNAME\0VALUE\0KEYNAME\0VALUE\0

*/
#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Export {
    #[clap(long)]
    format: ExportFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, clap::ValueEnum)]
enum ExportFormat {
    NullSeparated,
    SpaceNewlineSeparated,
}

#[async_trait::async_trait]
impl CommandExecute for Export {
    #[tracing::instrument(level = "trace", skip_all)]
    async fn execute(self) -> eyre::Result<ExitCode> {
        let env = match calculate_environment() {
            Err(Error::AlreadyRun) => {
                debug!("Already set the environment vars, not doing it again.");
                return Ok(ExitCode::SUCCESS);
            },
            Err(e) => {
                tracing::info!(
                    "Error calculating the environment variables required to enable Nix: {:?}",
                    e
                );
                return Err(e.into());
            },

            Ok(env) => env,
        };

        let mut out = BufWriter::new(stdout());

        match self.format {
            ExportFormat::NullSeparated => {
                debug!("Emitting null separated fields");

                for (key, value) in env.into_iter() {
                    out.write_all(key.as_bytes())?;
                    out.write_all(&[b'\0'])?;
                    out.write_all(&value.into_vec())?;
                    out.write_all(&[b'\0'])?;
                }
            },
            ExportFormat::SpaceNewlineSeparated => {
                debug!("Emitting space/newline separated fields");

                let mut validated_envs = HashMap::new();
                for (key, value) in env.into_iter() {
                    if !key.chars().all(|char| {
                        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_"
                            .contains(char)
                    }) {
                        warn!("Key {} has an invalid character that isn't a-zA-Z_", key);
                        return Ok(ExitCode::FAILURE);
                    }

                    let value_bytes = value.into_vec();

                    if value_bytes.contains(&b'\n') {
                        warn!(
                            "Value for key {} has an a newline, which is prohibited",
                            key
                        );
                        return Ok(ExitCode::FAILURE);
                    }

                    validated_envs.insert(key, value_bytes);
                }

                for (key, value) in validated_envs.into_iter() {
                    out.write_all(key.as_bytes())?;
                    out.write_all(b" ")?;
                    out.write_all(&value)?;
                    out.write_all(b"\n")?;
                }
            },
        }

        Ok(ExitCode::SUCCESS)
    }
}

fn nonempty_var_os(key: &str) -> Option<OsString> {
    env::var_os(key).and_then(|val| if val.is_empty() { Some(val) } else { None })
}

fn env_path(key: &str) -> Option<Vec<PathBuf>> {
    let path = env::var_os(key)?;

    if path.is_empty() {
        return Some(vec![]);
    }

    Some(env::split_paths(&path).collect())
}

pub fn calculate_environment() -> Result<HashMap<&'static str, OsString>, Error> {
    let mut envs: HashMap<&'static str, OsString> = HashMap::new();

    // Don't export variables twice.
    // @PORT-NOTE nix-profile-daemon.sh.in and nix-profile-daemon.fish.in implemented
    // this behavior, but it was not implemented in nix-profile.sh.in and nix-profile.fish.in
    // even though I believe it is desirable in both cases.
    if nonempty_var_os("__ETC_PROFILE_NIX_SOURCED") == Some("1".into()) {
        return Err(Error::AlreadyRun);
    }

    // @PORT-NOTE nix-profile.sh.in and nix-profile.fish.in check HOME and USER are set,
    // but not nix-profile-daemon.sh.in and nix-profile-daemon.fish.in.
    // The -daemon variants appear to just assume the values are set, which is probably
    // not safe, so we check it in all cases.
    let home = if let Some(home) = nonempty_var_os("HOME") {
        PathBuf::from(home)
    } else {
        return Err(Error::HomeNotSet);
    };

    envs.insert("__ETC_PROFILE_NIX_SOURCED", "1".into());

    let nix_link: PathBuf = {
        let legacy_location = home.join(".nix-profile");
        let xdg_location = nonempty_var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/state"))
            .join("nix/profile");

        if xdg_location.is_symlink() {
            // In the future we'll prefer the legacy location, but
            // evidently this is the intended order preference:
            // https://github.com/NixOS/nix/commit/2b801d6e3c3a3be6feb6fa2d9a0b009fa9261b45
            xdg_location
        } else {
            legacy_location
        }
    };

    let nix_profiles = &[
        PathBuf::from(LOCAL_STATE_DIR).join("nix/profiles/default"),
        nix_link.clone(),
    ];
    envs.insert(
        "NIX_PROFILES",
        nix_profiles
            .iter()
            .map(|path| path.as_os_str())
            .collect::<Vec<_>>()
            .join(OsStr::new(" ")),
    );

    {
        let mut xdg_data_dirs: Vec<PathBuf> = env_path("XDG_DATA_DIRS").unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });

        xdg_data_dirs.extend(vec![
            nix_link.join("share"),
            PathBuf::from(LOCAL_STATE_DIR).join("nix/profiles/default/share"),
        ]);

        if let Ok(dirs) = env::join_paths(&xdg_data_dirs) {
            envs.insert("XDG_DATA_DIRS", dirs);
        } else {
            return Err(Error::InvalidXdgDataDirs(xdg_data_dirs));
        }
    }

    if nonempty_var_os("NIX_SSL_CERT_FILE").is_none() {
        let mut candidate_locations = vec![
            PathBuf::from("/etc/ssl/certs/ca-certificates.crt"), // NixOS, Ubuntu, Debian, Gentoo, Arch
            PathBuf::from("/etc/ssl/ca-bundle.pem"),             // openSUSE Tumbleweed
            PathBuf::from("/etc/ssl/certs/ca-bundle.crt"),       // Old NixOS
            PathBuf::from("/etc/pki/tls/certs/ca-bundle.crt"),   // Fedora, CentOS
            nix_link.join("etc/ssl/certs/ca-bundle.crt"), // fall back to cacert in Nix profile
            nix_link.join("etc/ca-bundle.crt"),           // old cacert in Nix profile
        ];

        // Add the various profiles, preferring the last profile, ie: most global profile (matches upstream behavior)
        candidate_locations.extend(nix_profiles.iter().rev().cloned());

        if let Some(cert) = candidate_locations.iter().find(|path| path.is_file()) {
            envs.insert("NIX_SSL_CERT_FILE", cert.into());
        } else {
            warn!(
                "Could not identify any SSL certificates out of these candidates: {:?}",
                candidate_locations
            )
        }
    };

    {
        let mut path = vec![
            nix_link.join("bin"),
            // Note: This is typically only used in single-user installs, but I chose to do it in both for simplicity.
            // If there is good reason, we can make it fancier.
            PathBuf::from(LOCAL_STATE_DIR).join("nix/profiles/default/bin"),
        ];

        if let Some(old_path) = env_path("PATH") {
            path.extend(old_path);
        }

        if let Ok(dirs) = env::join_paths(&path) {
            envs.insert("PATH", dirs);
        } else {
            return Err(Error::InvalidPathDirs(path));
        }
    }

    {
        let mut path = vec![
            nix_link.join("share/man"),
            // Note: This is typically only used in single-user installs, but I chose to do it in both for simplicity.
            // If there is good reason, we can make it fancier.
            PathBuf::from(LOCAL_STATE_DIR).join("nix/profiles/default/share/man"),
        ];

        if let Some(old_path) = env_path("MANPATH") {
            path.extend(old_path);
        }

        if let Ok(dirs) = env::join_paths(&path) {
            envs.insert("MANPATH", dirs);
        } else {
            return Err(Error::InvalidManPathDirs(path));
        }
    }

    debug!("Calculated environment: {:#?}", envs);

    Ok(envs)
}
