mod error;
mod actions;
mod plan;
mod settings;

use std::{
    ffi::OsStr,
    fs::Permissions,
    io::SeekFrom,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::ExitStatus,
};

pub use error::HarmonicError;
pub use plan::InstallPlan;
pub use settings::InstallSettings;

use bytes::Buf;
use glob::glob;
use reqwest::Url;
use tempdir::TempDir;
use tokio::{
    io::{AsyncSeekExt, AsyncWriteExt},
    process::Command,
    task::spawn_blocking,
};

// This uses a Rust builder pattern
#[derive(Debug)]
pub struct Harmonic {
    dry_run: bool,
    daemon_user_count: usize,
    channels: Vec<(String, Url)>,
    modify_profile: bool,
    nix_build_group_name: String,
    nix_build_group_id: usize,
    nix_build_user_prefix: String,
    nix_build_user_id_base: usize,
}

impl Harmonic {
    pub fn dry_run(&mut self, dry_run: bool) -> &mut Self {
        self.dry_run = dry_run;
        self
    }
    pub fn daemon_user_count(&mut self, count: usize) -> &mut Self {
        self.daemon_user_count = count;
        self
    }

    pub fn channels(&mut self, channels: impl IntoIterator<Item = (String, Url)>) -> &mut Self {
        self.channels = channels.into_iter().collect();
        self
    }

    pub fn modify_profile(&mut self, toggle: bool) -> &mut Self {
        self.modify_profile = toggle;
        self
    }
}

impl Harmonic {
    #[tracing::instrument(skip_all)]
    pub async fn fetch_nix(&self) -> Result<(), HarmonicError> {
        tracing::info!("Fetching nix");
        // TODO(@hoverbear): architecture specific download
        // TODO(@hoverbear): hash check
        // TODO(@hoverbear): custom url
        let tempdir = TempDir::new("nix").map_err(HarmonicError::TempDir)?;
        fetch_url_and_unpack_xz(
            "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-x86_64-linux.tar.xz",
            tempdir.path(),
            self.dry_run,
        )
        .await?;

        let found_nix_path = if !self.dry_run {
            // TODO(@Hoverbear): I would like to make this less awful
            let found_nix_paths = glob::glob(&format!("{}/nix-*", tempdir.path().display()))?
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(
                found_nix_paths.len(),
                1,
                "Did not expect to find multiple nix paths, please report this"
            );
            found_nix_paths.into_iter().next().unwrap()
        } else {
            PathBuf::from("/nix/nix-*")
        };
        rename(found_nix_path.join("store"), "/nix/store", self.dry_run).await?;

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn create_group(&self) -> Result<(), HarmonicError> {
        tracing::info!("Creating group");
        execute_command(
            Command::new("groupadd")
                .arg("-g")
                .arg(self.nix_build_group_id.to_string())
                .arg("--system")
                .arg(&self.nix_build_group_name),
            self.dry_run,
        )
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn create_users(&self) -> Result<(), HarmonicError> {
        tracing::info!("Creating users");
        for index in 1..=self.daemon_user_count {
            let user_name = format!("{}{index}", self.nix_build_user_prefix);
            let user_id = self.nix_build_user_id_base + index;
            execute_command(
                Command::new("useradd").args([
                    "--home-dir",
                    "/var/empty",
                    "--comment",
                    &format!("\"Nix build user {user_id}\""),
                    "--gid",
                    &self.nix_build_group_name.to_string(),
                    "--groups",
                    &self.nix_build_group_name.to_string(),
                    "--no-user-group",
                    "--system",
                    "--shell",
                    "/sbin/nologin",
                    "--uid",
                    &user_id.to_string(),
                    "--password",
                    "\"!\"",
                    &user_name.to_string(),
                ]),
                self.dry_run,
            )
            .await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn create_directories(&self) -> Result<(), HarmonicError> {
        tracing::info!("Creating directories");
        create_directory("/nix", self.dry_run).await?;
        set_permissions(
            "/nix",
            None,
            Some("root".to_string()),
            Some(self.nix_build_group_name.clone()),
            self.dry_run,
        )
        .await?;

        let permissions = Permissions::from_mode(0o0755);
        let paths = [
            "/nix/var",
            "/nix/var/log",
            "/nix/var/log/nix",
            "/nix/var/log/nix/drvs",
            "/nix/var/nix",
            "/nix/var/nix/db",
            "/nix/var/nix/gcroots",
            "/nix/var/nix/gcroots/per-user",
            "/nix/var/nix/profiles",
            "/nix/var/nix/profiles/per-user",
            "/nix/var/nix/temproots",
            "/nix/var/nix/userpool",
            "/nix/var/nix/daemon-socket",
        ];

        for path in paths {
            // We use `create_dir` over `create_dir_all` to ensure we always set permissions right
            create_directory(path, self.dry_run).await?;
            set_permissions(path, Some(permissions.clone()), None, None, self.dry_run).await?;
        }
        create_directory("/nix/store", self.dry_run).await?;
        set_permissions(
            "/nix/store",
            Some(Permissions::from_mode(0o1775)),
            None,
            Some(self.nix_build_group_name.clone()),
            self.dry_run,
        )
        .await?;
        create_directory("/etc/nix", self.dry_run).await?;
        set_permissions(
            "/etc/nix",
            Some(Permissions::from_mode(0o0555)),
            None,
            None,
            self.dry_run,
        )
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn place_channel_configuration(&self) -> Result<(), HarmonicError> {
        tracing::info!("Placing channel configuration");
        let buf = self
            .channels
            .iter()
            .map(|(name, url)| format!("{} {}", url, name))
            .collect::<Vec<_>>()
            .join("\n");

        create_file_if_not_exists("/root/.nix-channels", buf, self.dry_run).await?;
        set_permissions(
            "/root/.nix-channels",
            Some(Permissions::from_mode(0o0664)),
            None,
            None,
            self.dry_run,
        )
        .await
    }

    #[tracing::instrument(skip_all)]
    pub async fn configure_shell_profile(&self) -> Result<(), HarmonicError> {
        tracing::info!("Configuring shell profile");
        const PROFILE_TARGETS: &[&str] = &[
            "/etc/bashrc",
            "/etc/profile.d/nix.sh",
            "/etc/zshrc",
            "/etc/bash.bashrc",
            "/etc/zsh/zshrc",
        ];
        const PROFILE_NIX_FILE: &str = "/nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh";
        for profile_target in PROFILE_TARGETS {
            let path = Path::new(profile_target);
            let buf = format!(
                "\n\
                # Nix\n\
                if [ -e '{PROFILE_NIX_FILE}' ]; then\n\
                . '{PROFILE_NIX_FILE}'\n\
                fi\n\
                # End Nix\n
            \n",
            );
            create_or_append_file(path, buf, self.dry_run).await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn setup_default_profile(&self) -> Result<(), HarmonicError> {
        tracing::info!("Setting up default profile");
        // Find an `nix` package
        let nix_pkg_glob = "/nix/store/*-nix-*";
        let found_nix_pkg = if !self.dry_run {
            let mut found_pkg = None;
            for entry in glob(nix_pkg_glob).map_err(HarmonicError::GlobPatternError)? {
                match entry {
                    Ok(path) => {
                        // TODO(@Hoverbear): Should probably ensure is unique
                        found_pkg = Some(path);
                        break;
                    }
                    Err(_) => continue, /* Ignore it */
                };
            }
            found_pkg
        } else {
            // This is a mock for dry running.
            Some(PathBuf::from(nix_pkg_glob))
        };
        let nix_pkg = if let Some(nix_pkg) = found_nix_pkg {
            nix_pkg
        } else {
            return Err(HarmonicError::NoNssCacert);
        };

        execute_command(
            Command::new(nix_pkg.join("bin/nix-env"))
                .arg("-i")
                .arg(&nix_pkg),
            self.dry_run,
        )
        .await?;

        // Find an `nss-cacert` package, add it too.
        let nss_ca_cert_pkg_glob = "/nix/store/*-nss-cacert-*";
        let found_nss_ca_cert_pkg = if !self.dry_run {
            let mut found_pkg = None;
            for entry in glob(nss_ca_cert_pkg_glob).map_err(HarmonicError::GlobPatternError)? {
                match entry {
                    Ok(path) => {
                        // TODO(@Hoverbear): Should probably ensure is unique
                        found_pkg = Some(path);
                        break;
                    }
                    Err(_) => continue, /* Ignore it */
                };
            }
            found_pkg
        } else {
            // This is a mock for dry running.
            Some(PathBuf::from(nss_ca_cert_pkg_glob))
        };

        if let Some(nss_ca_cert_pkg) = found_nss_ca_cert_pkg {
            execute_command(
                Command::new(nix_pkg.join("bin/nix-env"))
                    .arg("-i")
                    .arg(&nss_ca_cert_pkg),
                self.dry_run,
            )
            .await?;
            set_env(
                "NIX_SSL_CERT_FILE",
                "/nix/var/nix/profiles/default/etc/ssl/certs/ca-bundle.crt",
                self.dry_run,
            );
            nss_ca_cert_pkg
        } else {
            return Err(HarmonicError::NoNssCacert);
        };
        if !self.channels.is_empty() {
            execute_command(
                Command::new(nix_pkg.join("bin/nix-channel"))
                    .arg("--update")
                    .arg("nixpkgs")
                    .env(
                        "NIX_SSL_CERT_FILE",
                        "/nix/var/nix/profiles/default/etc/ssl/certs/ca-bundle.crt",
                    ),
                self.dry_run,
            )
            .await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    pub async fn place_nix_configuration(&self) -> Result<(), HarmonicError> {
        tracing::info!("Placing nix configuration");
        let buf = format!(
            "\
            {extra_conf}\n\
            build-users-group = {build_group_name}\n\
        ",
            extra_conf = "", // TODO(@Hoverbear): populate me
            build_group_name = self.nix_build_group_name,
        );
        create_file_if_not_exists("/etc/nix/nix.conf", buf, self.dry_run).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn configure_nix_daemon_service(&self) -> Result<(), HarmonicError> {
        tracing::info!("Configuring nix daemon service");
        if Path::new("/run/systemd/system").exists() {
            const SERVICE_SRC: &str =
                "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.service";

            const SOCKET_SRC: &str =
                "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.socket";

            const TMPFILES_SRC: &str =
                "/nix/var/nix/profiles/default//lib/tmpfiles.d/nix-daemon.conf";
            const TMPFILES_DEST: &str = "/etc/tmpfiles.d/nix-daemon.conf";

            symlink(TMPFILES_SRC, TMPFILES_DEST, self.dry_run).await?;
            execute_command(
                Command::new("systemd-tmpfiles")
                    .arg("--create")
                    .arg("--prefix=/nix/var/nix"),
                self.dry_run,
            )
            .await?;
            execute_command(
                Command::new("systemctl").arg("link").arg(SERVICE_SRC),
                self.dry_run,
            )
            .await?;
            execute_command(
                Command::new("systemctl").arg("enable").arg(SOCKET_SRC),
                self.dry_run,
            )
            .await?;
            // TODO(@Hoverbear): Handle proxy vars
            execute_command(Command::new("systemctl").arg("daemon-reload"), self.dry_run).await?;
            execute_command(
                Command::new("systemctl")
                    .arg("start")
                    .arg("nix-daemon.socket"),
                self.dry_run,
            )
            .await?;
            execute_command(
                Command::new("systemctl")
                    .arg("restart")
                    .arg("nix-daemon.service"),
                self.dry_run,
            )
            .await?;
        } else {
            return Err(HarmonicError::InitNotSupported);
        }
        Ok(())
    }
}

impl Default for Harmonic {
    fn default() -> Self {
        Self {
            dry_run: true,
            channels: vec![(
                "nixpkgs".to_string(),
                "https://nixos.org/channels/nixpkgs-unstable"
                    .parse::<Url>()
                    .unwrap(),
            )],
            daemon_user_count: 32,
            modify_profile: true,
            nix_build_group_name: String::from("nixbld"),
            nix_build_group_id: 30000,
            nix_build_user_prefix: String::from("nixbld"),
            nix_build_user_id_base: 30000,
        }
    }
}

#[tracing::instrument(skip_all, fields(
    path = %path.as_ref().display(),
    permissions = tracing::field::valuable(&permissions.clone().map(|v| format!("{:#o}", v.mode()))),
    owner = tracing::field::valuable(&owner),
    group = tracing::field::valuable(&group),
))]
async fn set_permissions(
    path: impl AsRef<Path>,
    permissions: Option<Permissions>,
    owner: Option<String>,
    group: Option<String>,
    dry_run: bool,
) -> Result<(), HarmonicError> {
    use nix::unistd::{chown, Group, User};
    use walkdir::WalkDir;
    if !dry_run {
        tracing::trace!("Setting permissions");
        let path = path.as_ref();
        let uid = if let Some(owner) = owner {
            let uid = User::from_name(owner.as_str())
                .map_err(|e| HarmonicError::UserId(owner.clone(), e))?
                .ok_or(HarmonicError::NoUser(owner))?
                .uid;
            Some(uid)
        } else {
            None
        };
        let gid = if let Some(group) = group {
            let gid = Group::from_name(group.as_str())
                .map_err(|e| HarmonicError::GroupId(group.clone(), e))?
                .ok_or(HarmonicError::NoGroup(group))?
                .gid;
            Some(gid)
        } else {
            None
        };
        for child in WalkDir::new(path) {
            let entry = child.map_err(|e| HarmonicError::WalkDirectory(path.to_owned(), e))?;
            if let Some(ref perms) = permissions {
                tokio::fs::set_permissions(path, perms.clone())
                    .await
                    .map_err(|e| HarmonicError::SetPermissions(path.to_owned(), e))?;
            }
            chown(entry.path(), uid, gid)
                .map_err(|e| HarmonicError::Chown(entry.path().to_owned(), e))?;
        }
    } else {
        tracing::info!("Dry run: Would recursively set permissions/ownership");
    }

    Ok(())
}

#[tracing::instrument(skip_all, fields(
    path = %path.as_ref().display(),
))]
async fn create_directory(path: impl AsRef<Path>, dry_run: bool) -> Result<(), HarmonicError> {
    use tokio::fs::create_dir;
    if !dry_run {
        tracing::trace!("Creating directory");
        let path = path.as_ref();
        create_dir(path)
            .await
            .map_err(|e| HarmonicError::CreateDirectory(path.to_owned(), e))?;
    } else {
        tracing::info!("Dry run: Would create directory");
    }

    Ok(())
}

#[tracing::instrument(skip_all, fields(command = %format!("{:?}", command.as_std())))]
async fn execute_command(
    command: &mut Command,
    dry_run: bool,
) -> Result<ExitStatus, HarmonicError> {
    if !dry_run {
        tracing::trace!("Executing");
        let command_str = format!("{:?}", command.as_std());
        let status = command
            .status()
            .await
            .map_err(|e| HarmonicError::CommandFailedExec(command_str.clone(), e))?;
        match status.success() {
            true => Ok(status),
            false => Err(HarmonicError::CommandFailedStatus(command_str)),
        }
    } else {
        tracing::info!("Dry run: Would execute");
        // You cannot conjure "good" exit status in Rust without breaking the rules
        // So... we conjure one from `true`
        Command::new("true")
            .status()
            .await
            .map_err(|e| HarmonicError::CommandFailedExec(String::from("true"), e))
    }
}

#[tracing::instrument(skip_all, fields(
    path = %path.as_ref().display(),
    buf = %format!("```{}```", buf.as_ref()),
))]
async fn create_or_append_file(
    path: impl AsRef<Path>,
    buf: impl AsRef<str>,
    dry_run: bool,
) -> Result<(), HarmonicError> {
    use tokio::fs::{create_dir_all, OpenOptions};
    let path = path.as_ref();
    let buf = buf.as_ref();
    if !dry_run {
        tracing::trace!("Creating or appending");
        if let Some(parent) = path.parent() {
            create_dir_all(parent)
                .await
                .map_err(|e| HarmonicError::CreateDirectory(parent.to_owned(), e))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| HarmonicError::OpenFile(path.to_owned(), e))?;

        file.seek(SeekFrom::End(0))
            .await
            .map_err(|e| HarmonicError::SeekFile(path.to_owned(), e))?;
        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| HarmonicError::WriteFile(path.to_owned(), e))?;
    } else {
        tracing::info!("Dry run: Would create or append");
    }
    Ok(())
}

#[tracing::instrument(skip_all, fields(
    path = %path.as_ref().display(),
    buf = %format!("```{}```", buf.as_ref()),
))]
async fn create_file_if_not_exists(
    path: impl AsRef<Path>,
    buf: impl AsRef<str>,
    dry_run: bool,
) -> Result<(), HarmonicError> {
    use tokio::fs::{create_dir_all, OpenOptions};
    let path = path.as_ref();
    let buf = buf.as_ref();
    if !dry_run {
        tracing::trace!("Creating if not exists");
        if let Some(parent) = path.parent() {
            create_dir_all(parent)
                .await
                .map_err(|e| HarmonicError::CreateDirectory(parent.to_owned(), e))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| HarmonicError::OpenFile(path.to_owned(), e))?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| HarmonicError::WriteFile(path.to_owned(), e))?;
    } else {
        tracing::info!("Dry run: Would create (or error if exists)");
    }
    Ok(())
}

#[tracing::instrument(skip_all, fields(
    src = %src.as_ref().display(),
    dest = %dest.as_ref().display(),
))]
async fn symlink(
    src: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    dry_run: bool,
) -> Result<(), HarmonicError> {
    let src = src.as_ref();
    let dest = dest.as_ref();
    if !dry_run {
        tracing::trace!("Symlinking");
        tokio::fs::symlink(src, dest)
            .await
            .map_err(|e| HarmonicError::Symlink(src.to_owned(), dest.to_owned(), e))?;
    } else {
        tracing::info!("Dry run: Would symlink",);
    }
    Ok(())
}

#[tracing::instrument(skip_all, fields(
    src = %src.as_ref().display(),
    dest = %dest.as_ref().display(),
))]
async fn rename(
    src: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    dry_run: bool,
) -> Result<(), HarmonicError> {
    let src = src.as_ref();
    let dest = dest.as_ref();
    if !dry_run {
        tracing::trace!("Renaming");
        tokio::fs::rename(src, dest)
            .await
            .map_err(|e| HarmonicError::Rename(src.to_owned(), dest.to_owned(), e))?;
    } else {
        tracing::info!("Dry run: Would rename",);
    }
    Ok(())
}

#[tracing::instrument(skip_all, fields(
    url = %url.as_ref(),
    dest = %dest.as_ref().display(),
))]
async fn fetch_url_and_unpack_xz(
    url: impl AsRef<str>,
    dest: impl AsRef<Path>,
    dry_run: bool,
) -> Result<(), HarmonicError> {
    let url = url.as_ref();
    let dest = dest.as_ref().to_owned();
    if !dry_run {
        tracing::trace!("Fetching url");
        let res = reqwest::get(url).await.map_err(HarmonicError::Reqwest)?;
        let bytes = res.bytes().await.map_err(HarmonicError::Reqwest)?;
        // TODO(@Hoverbear): Pick directory
        tracing::trace!("Unpacking tar.xz");
        let handle: Result<(), HarmonicError> = spawn_blocking(move || {
            let decoder = xz2::read::XzDecoder::new(bytes.reader());
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&dest).map_err(HarmonicError::Unarchive)?;
            tracing::debug!(dest = %dest.display(), "Downloaded & extracted Nix");
            Ok(())
        })
        .await?;

        handle?;
    } else {
        tracing::info!("Dry run: Would fetch and unpack xz tarball");
    }

    Ok(())
}

#[tracing::instrument(skip_all, fields(
    k = %k.as_ref().to_string_lossy(),
    v = %v.as_ref().to_string_lossy(),
))]
fn set_env(k: impl AsRef<OsStr>, v: impl AsRef<OsStr>, dry_run: bool) {
    if !dry_run {
        tracing::trace!("Setting env");
        std::env::set_var(k.as_ref(), v.as_ref());
    } else {
        tracing::info!("Dry run: Would set env");
    }
}
