use std::path::{Path, PathBuf};

use nix::unistd::{chown, Group, User};
use serde::Serialize;
use tokio::fs::create_dir;

use crate::HarmonicError;

use crate::actions::{ActionDescription, Actionable, ActionState, Action};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateDirectory {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
}

impl CreateDirectory {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: String,
        group: String,
        mode: u32,
        force: bool,
    ) -> Result<Self, HarmonicError> {
        let path = path.as_ref();

        if path.exists() && !force {
            return Err(HarmonicError::CreateDirectory(
                path.to_path_buf(),
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("Directory `{}` already exists", path.display()),
                ),
            ));
        }

        Ok(Self {
            path: path.to_path_buf(),
            user,
            group,
            mode,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<CreateDirectory> {
    type Error = CreateDirectoryError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user,
            group,
            mode,
        } = &self;
        vec![ActionDescription::new(
            format!("Create the directory `{}`", path.display()),
            vec![format!(
                "Creating directory `{}` owned by `{user}:{group}` with mode `{mode:#o}`",
                path.display()
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
        } = self;

        let gid = Group::from_name(group.as_str())
            .map_err(|e| HarmonicError::GroupId(group.clone(), e))?
            .ok_or(HarmonicError::NoGroup(group.clone()))?
            .gid;
        let uid = User::from_name(user.as_str())
            .map_err(|e| HarmonicError::UserId(user.clone(), e))?
            .ok_or(HarmonicError::NoUser(user.clone()))?
            .uid;

        tracing::trace!(path = %path.display(), "Creating directory");
        create_dir(path.clone())
            .await
            .map_err(|e| HarmonicError::CreateDirectory(path.clone(), e))?;
        chown(&path, Some(uid), Some(gid)).map_err(|e| HarmonicError::Chown(path.clone(), e))?;

        Ok(())
    }


    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}


impl From<ActionState<CreateDirectory>> for ActionState<Action> {
    fn from(v: ActionState<CreateDirectory>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::CreateDirectory(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::CreateDirectory(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::CreateDirectory(v)),
        }
    }
}


#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateDirectoryError {

}
