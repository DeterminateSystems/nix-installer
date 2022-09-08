mod error;
use std::{
    fs::Permissions,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::ExitStatus,
};

pub use error::HarmonicError;

#[cfg(target_os = "linux")]
mod nixos;
#[cfg(target_os = "linux")]
pub use nixos::NixOs;

use bytes::Buf;
use glob::glob;
use reqwest::Url;
use tokio::{
    fs::{create_dir, create_dir_all, set_permissions, symlink, OpenOptions},
    io::AsyncWriteExt,
    process::Command,
    task::spawn_blocking,
};

// This uses a Rust builder pattern
#[derive(Debug)]
pub struct Harmonic {
    daemon_user_count: usize,
    channels: Vec<Url>,
    modify_profile: bool,
    nix_build_group_name: String,
    nix_build_group_id: usize,
    nix_build_user_prefix: String,
    nix_build_user_id_base: usize,
}

impl Harmonic {
    pub fn daemon_user_count(&mut self, count: usize) -> &mut Self {
        self.daemon_user_count = count;
        self
    }

    pub fn channels(&mut self, channels: impl IntoIterator<Item = Url>) -> &mut Self {
        self.channels = channels.into_iter().collect();
        self
    }

    pub fn modify_profile(&mut self, toggle: bool) -> &mut Self {
        self.modify_profile = toggle;
        self
    }
}

impl Harmonic {
    pub async fn fetch_nix(&self) -> Result<(), HarmonicError> {
        // TODO(@hoverbear): architecture specific download
        // TODO(@hoverbear): hash check
        let res = reqwest::get(
            "https://releases.nixos.org/nix/nix-2.11.0/nix-2.11.0-x86_64-linux.tar.xz",
        )
        .await
        .map_err(HarmonicError::DownloadingNix)?;
        let bytes = res.bytes().await.map_err(HarmonicError::DownloadingNix)?;
        // TODO(@Hoverbear): Pick directory
        let handle: Result<(), HarmonicError> = spawn_blocking(|| {
            let decoder = xz2::read::XzDecoder::new(bytes.reader());
            let mut archive = tar::Archive::new(decoder);
            let destination = "/nix/install";
            archive
                .unpack(destination)
                .map_err(HarmonicError::UnpackingNix)?;
            tracing::debug!(%destination, "Downloaded & extracted Nix");
            Ok(())
        })
        .await?;

        handle?;

        Ok(())
    }

    pub async fn create_group(&self) -> Result<(), HarmonicError> {
        let status = Command::new("groupadd")
            .arg("-g")
            .arg(self.nix_build_group_id.to_string())
            .arg("--system")
            .arg(&self.nix_build_group_name)
            .status()
            .await
            .map_err(HarmonicError::GroupAddSpawn)?;
        if !status.success() {
            Err(HarmonicError::GroupAddFailure(status))
        } else {
            Ok(())
        }
    }

    pub async fn create_users(&self) -> Result<(), HarmonicError> {
        for index in 1..=self.daemon_user_count {
            let user_name = format!("{}{index}", self.nix_build_user_prefix);
            let user_id = self.nix_build_user_id_base + index;
            let status = Command::new("useradd")
                .args([
                    "--home-dir",
                    "/var/empty",
                    "--comment",
                    &format!("\"Nix build user {user_id}\""),
                    "--gid",
                    &self.nix_build_group_id.to_string(),
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
                ])
                .status()
                .await
                .map_err(HarmonicError::UserAddSpawn)?;
            if !status.success() {
                return Err(HarmonicError::UserAddFailure(status));
            }
        }
        Ok(())
    }
    pub async fn create_directories(&self) -> Result<(), HarmonicError> {
        let permissions = Permissions::from_mode(0o755);
        let paths = [
            "/nix",
            "/nix/var",
            "/nix/var/log",
            "/nix/var/log/nix",
            "/nix/var/log/nix/drvs",
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
            create_dir_with_permissions(path, permissions.clone())
                .await
                .map_err(HarmonicError::CreateDirectory)?;
        }
        Ok(())
    }

    pub async fn place_channel_configuration(&self) -> Result<(), HarmonicError> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open("/root/.nix-channels") // TODO(@hoverbear): We should figure out the actual root dir
            .await
            .map_err(HarmonicError::PlaceChannelConfiguration)?;

        let buf = self
            .channels
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        file.write_all(buf.as_bytes())
            .await
            .map_err(HarmonicError::PlaceChannelConfiguration)?;
        Ok(())
    }
    pub async fn configure_shell_profile(&self) -> Result<(), HarmonicError> {
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
                fi\n
                # End Nix\n
            \n",
            );
            if path.exists() {
                // TODO(@Hoverbear): Backup
                // TODO(@Hoverbear): See if the line already exists, skip setting it
                tracing::trace!("TODO");
            } else if let Some(parent) = path.parent() {
                create_dir_all(parent).await.unwrap()
            }
            let mut file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(profile_target)
                .await
                .map_err(|e| HarmonicError::OpeningFile(path.to_owned(), e))?;
            file.write_all(buf.as_bytes())
                .await
                .map_err(|e| HarmonicError::WritingFile(path.to_owned(), e))?;
        }
        Ok(())
    }

    pub async fn setup_default_profile(&self) -> Result<(), HarmonicError> {
        Command::new("/nix/install/bin/nix-env")
            .arg("-i")
            .arg("/nix/install")
            .status()
            .await
            .map_err(HarmonicError::InstallNixIntoStore)?;
        // Find an `nss-cacert` package, add it too.
        let mut found_nss_ca_cert = None;
        for entry in
            glob("/nix/install/store/*-nss-cacert").map_err(HarmonicError::GlobPatternError)?
        {
            match entry {
                Ok(path) => {
                    // TODO(@Hoverbear): Should probably ensure is unique
                    found_nss_ca_cert = Some(path);
                    break;
                }
                Err(_) => continue, /* Ignore it */
            };
        }
        if let Some(nss_ca_cert) = found_nss_ca_cert {
            let status = Command::new("/nix/install/bin/nix-env")
                .arg("-i")
                .arg(&nss_ca_cert)
                .status()
                .await
                .map_err(HarmonicError::InstallNssCacertIntoStore)?;
            if !status.success() {
                // TODO(@Hoverbear): report
            }
            std::env::set_var("NIX_SSL_CERT_FILE", &nss_ca_cert);
        } else {
            return Err(HarmonicError::NoNssCacert);
        }
        if !self.channels.is_empty() {
            status_failure_as_error(
                Command::new("/nix/install/bin/nix-channel")
                    .arg("--update")
                    .arg("nixpkgs"),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn place_nix_configuration(&self) -> Result<(), HarmonicError> {
        let mut nix_conf = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open("/etc/nix/nix.conf")
            .await
            .map_err(HarmonicError::CreatingNixConf)?;
        let buf = format!(
            "\
            {extra_conf}\n\
            build-users-group = {build_group_name}\n\
        ",
            extra_conf = "", // TODO(@Hoverbear): populate me
            build_group_name = self.nix_build_group_name,
        );
        nix_conf
            .write_all(buf.as_bytes())
            .await
            .map_err(HarmonicError::CreatingNixConf)?;

        Ok(())
    }

    pub async fn configure_nix_daemon_service(&self) -> Result<(), HarmonicError> {
        if Path::new("/run/systemd/system").exists() {
            const SERVICE_SRC: &str =
                "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.service";

            const SOCKET_SRC: &str =
                "/nix/var/nix/profiles/default/lib/systemd/system/nix-daemon.socket";

            const TMPFILES_SRC: &str =
                "/nix/var/nix/profiles/default//lib/tmpfiles.d/nix-daemon.conf";
            const TMPFILES_DEST: &str = "/etc/tmpfiles.d/nix-daemon.conf";

            symlink(TMPFILES_SRC, TMPFILES_DEST).await.map_err(|e| {
                HarmonicError::Linking(PathBuf::from(TMPFILES_SRC), PathBuf::from(TMPFILES_DEST), e)
            })?;
            status_failure_as_error(
                Command::new("systemd-tmpfiles")
                    .arg("--create")
                    .arg("--prefix=/nix/var/nix"),
            )
            .await?;
            status_failure_as_error(Command::new("systemctl").arg("link").arg(SERVICE_SRC)).await?;
            status_failure_as_error(Command::new("systemctl").arg("enable").arg(SOCKET_SRC))
                .await?;
            // TODO(@Hoverbear): Handle proxy vars
            status_failure_as_error(Command::new("systemctl").arg("daemon-reload")).await?;
            status_failure_as_error(
                Command::new("systemctl")
                    .arg("start")
                    .arg("nix-daemon.socket"),
            )
            .await?;
            status_failure_as_error(
                Command::new("systemctl")
                    .arg("restart")
                    .arg("nix-daemon.service"),
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
            channels: vec!["https://nixos.org/channels/nixpkgs-unstable"
                .parse::<Url>()
                .unwrap()],
            daemon_user_count: 32,
            modify_profile: true,
            nix_build_group_name: String::from("nixbld"),
            nix_build_group_id: 30000,
            nix_build_user_prefix: String::from("nixbld"),
            nix_build_user_id_base: 30000,
        }
    }
}

async fn create_dir_with_permissions(
    path: impl AsRef<Path>,
    permissions: Permissions,
) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    create_dir(path).await?;
    set_permissions(path, permissions).await?;
    Ok(())
}

async fn status_failure_as_error(command: &mut Command) -> Result<ExitStatus, HarmonicError> {
    let command_str = format!("{:?}", command.as_std());
    let status = command
        .status()
        .await
        .map_err(|e| HarmonicError::CommandFailedExec(command_str.clone(), e))?;
    match status.success() {
        true => Ok(status),
        false => Err(HarmonicError::CommandFailedStatus(command_str)),
    }
}
