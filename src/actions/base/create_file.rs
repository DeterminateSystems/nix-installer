use std::{
    fs::Permissions,
    path::{Path, PathBuf, self}, io::SeekFrom,
};
use nix::unistd::{Group, User, Gid, Uid, chown};
use tokio::{fs::{create_dir, create_dir_all, OpenOptions}, io::{AsyncWriteExt, AsyncSeekExt}};

use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateFile {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
    buf: String,
}

impl CreateFile {
    pub async fn plan(path: impl AsRef<Path>, user: String, group: String, mode: u32, buf: String) -> Result<Self, HarmonicError> {
        let path = path.as_ref().to_path_buf();

        Ok(Self { path, user, group, mode, buf })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateFile {
    type Receipt = CreateFileReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { path, user, group, mode, buf } = &self;
        vec![ActionDescription::new(
            format!("Create or overwrite file `{}`", path.display()),
            vec![format!(
                "Create or overwrite `{}` owned by `{user}:{group}` with mode `{mode:#o}` with `{buf}`", path.display()
            )],
        )]
    }

    async fn execute(self) -> Result<CreateFileReceipt, HarmonicError> {
        let Self { path, user, group, mode, buf } = self;

        tracing::trace!("Creating or appending");
        if let Some(parent) = path.parent() {
            create_dir_all(parent)
                .await
                .map_err(|e| HarmonicError::CreateDirectory(parent.to_owned(), e))?;
        }
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
        chown(&path, Some(uid), Some(gid))
            .map_err(|e| HarmonicError::Chown(path.clone(), e))?;
        
        Ok(Self::Receipt { path, user, group, mode, buf })
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

    async fn revert(self) -> Result<(), HarmonicError> {


        todo!();

        Ok(())
    }
}
