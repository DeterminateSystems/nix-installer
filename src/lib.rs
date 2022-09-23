mod actions;
mod error;
mod plan;
mod settings;

use std::{
    ffi::OsStr,
    fs::Permissions,
    io::SeekFrom,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::ExitStatus, fmt::Display,
};

pub use error::HarmonicError;
pub use plan::InstallPlan;
use serde::Serializer;
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
) -> Result<ExitStatus, std::io::Error> {
    tracing::trace!("Executing");
    let command_str = format!("{:?}", command.as_std());
    let status = command
        .status()
        .await?;
    match status.success() {
        true => Ok(status),
        false => Err(std::io::Error::new(std::io::ErrorKind::Other, "Failed status")),
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

fn serialize_error_to_display<E, S>(err: &E, ser: S) -> Result<S::Ok, S::Error> where E: Display, S: Serializer {
    ser.serialize_str(&format!("{err:#}"))
}