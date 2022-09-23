use nix::unistd::{chown, Group, User};
use serde::Serialize;
use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
};
use tokio::{
    fs::{create_dir_all, OpenOptions},
    io::{AsyncSeekExt, AsyncWriteExt},
};

use crate::{HarmonicError, actions::{ActionState, Action}};

use crate::actions::{ActionDescription, Actionable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateOrAppendFile {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
    buf: String,
}

impl CreateOrAppendFile {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: String,
        group: String,
        mode: u32,
        buf: String,
    ) -> Result<Self, HarmonicError> {
        let path = path.as_ref().to_path_buf();

        Ok(Self {
            path,
            user,
            group,
            mode,
            buf,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<CreateOrAppendFile> {
    type Error = CreateOrAppendFileError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
        } = &self;
        vec![ActionDescription::new(
            format!("Create or append file `{}`", path.display()),
            vec![format!(
                "Create or append `{}` owned by `{user}:{group}` with mode `{mode:#o}` with `{buf}`", path.display()
            )],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
        } = self;

        tracing::trace!(path = %path.display(), "Creating or appending");
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

        let gid = Group::from_name(group.as_str())
            .map_err(|e| HarmonicError::GroupId(group.clone(), e))?
            .ok_or(HarmonicError::NoGroup(group.clone()))?
            .gid;
        let uid = User::from_name(user.as_str())
            .map_err(|e| HarmonicError::UserId(user.clone(), e))?
            .ok_or(HarmonicError::NoUser(user.clone()))?
            .uid;

            tracing::trace!(path = %path.display(), "Chowning");
        chown(&path, Some(uid), Some(gid)).map_err(|e| HarmonicError::Chown(path.clone(), e))?;

        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}


impl From<ActionState<CreateOrAppendFile>> for ActionState<Action> {
    fn from(v: ActionState<CreateOrAppendFile>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::CreateOrAppendFile(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::CreateOrAppendFile(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::CreateOrAppendFile(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateOrAppendFileError {

}
