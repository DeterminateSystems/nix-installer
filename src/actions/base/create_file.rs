use nix::unistd::{chown, Group, User};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::{
    fs::{create_dir_all, OpenOptions},
    io::AsyncWriteExt,
};

use crate::{HarmonicError, actions::{ActionState, Action}};

use crate::actions::{ActionDescription, Actionable};

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
impl Actionable for ActionState<CreateFile> {
    type Error = CreateFileError;
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
    async fn execute(&mut self) -> Result<(), Self::Error> {
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

        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<CreateFile>> for ActionState<Action> {
    fn from(v: ActionState<CreateFile>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::CreateFile(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::CreateFile(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::CreateFile(v)),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateFileError {

}
