mod error;
use std::{fs::Permissions, os::unix::fs::PermissionsExt, path::Path};

pub use error::HarmonicError;

#[cfg(target_os = "linux")]
mod nixos;
#[cfg(target_os = "linux")]
pub use nixos::NixOs;

use futures::stream::TryStreamExt;
use reqwest::Url;
use tokio::{
    fs::{create_dir, set_permissions, OpenOptions},
    io::AsyncWriteExt,
    process::Command,
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
        let stream = res.bytes_stream();
        let async_read = stream
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            .into_async_read();
        let buffered = futures::io::BufReader::new(async_read);
        let decoder = async_compression::futures::bufread::XzDecoder::new(buffered);
        let archive = async_tar::Archive::new(decoder);

        // TODO(@Hoverbear): Pick directory
        let destination = "/nix/store";
        archive
            .unpack(destination)
            .await
            .map_err(HarmonicError::UnpackingNix)?;
        tracing::debug!(%destination, "Downloaded & extracted Nix");
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
        let permissions = Permissions::from_mode(755);
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
        todo!();
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
