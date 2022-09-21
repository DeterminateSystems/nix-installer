use nix::unistd::{chown, Group, User};
use std::path::{Path, PathBuf};
use tokio::{
    fs::{create_dir_all, OpenOptions},
    io::AsyncWriteExt,
};

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateFile {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
    buf: String,
    force: bool,
}

impl CreateFile {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: String,
        group: String,
        mode: u32,
        buf: String,
        force: bool,
    ) -> Result<Self, HarmonicError> {
        let path = path.as_ref().to_path_buf();

        if path.exists() && !force {
            return Err(HarmonicError::CreateFile(
                path.to_path_buf(),
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("Directory `{}` already exists", path.display()),
                ),
            ));
        }

        Ok(Self {
            path,
            user,
            group,
            mode,
            buf,
            force,
        })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateFile {
    type Receipt = CreateFileReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
            force,
        } = &self;
        vec![ActionDescription::new(
            format!("Create or overwrite file `{}`", path.display()),
            vec![format!(
                "Create or overwrite `{}` owned by `{user}:{group}` with mode `{mode:#o}` with `{buf}`", path.display()
            )],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<CreateFileReceipt, HarmonicError> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
            force: _,
        } = self;
        tracing::trace!(path = %path.display(), "Creating file");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(&path)
            .await
            .map_err(|e| HarmonicError::OpenFile(path.to_owned(), e))?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| HarmonicError::WriteFile(path.to_owned(), e))?;

        let gid = Group::from_name(group.as_str())
            .map_err(|e| HarmonicError::GroupId(group.clone(), e))?
            .ok_or(HarmonicError::NoGroup(group.clone()))?
            .gid;
        let uid = User::from_name(user.as_str())
            .map_err(|e| HarmonicError::UserId(user.clone(), e))?
            .ok_or(HarmonicError::NoUser(user.clone()))?
            .uid;
        
        tracing::trace!(path = %path.display(), "Chowning file");
        chown(&path, Some(uid), Some(gid)).map_err(|e| HarmonicError::Chown(path.clone(), e))?;

        Ok(Self::Receipt {
            path,
            user,
            group,
            mode,
            buf,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateFileReceipt {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
    buf: String,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateFileReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
