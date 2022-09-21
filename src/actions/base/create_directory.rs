use std::{
    fs::Permissions,
    path::{Path, PathBuf},
};

use nix::unistd::{Group, User, Gid, Uid, chown};
use tokio::fs::create_dir;

use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateDirectory {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
}

impl CreateDirectory {
    pub async fn plan(path: impl AsRef<Path>, user: String, group: String, mode: u32, force: bool) -> Result<Self, HarmonicError> {
        let path = path.as_ref();

        if path.exists() && !force {
            return Err(HarmonicError::CreateDirectory(path.to_path_buf(), std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("Directory `{}` already exists", path.display()))))
        }
        // Ensure the group/user exist, we don't store them since we really need to serialize them
        let _has_gid = Group::from_name(group.as_str())
                .map_err(|e| HarmonicError::GroupId(group.clone(), e))?
                .ok_or(HarmonicError::NoGroup(group.clone()))?;
        let _has_uid = User::from_name(user.as_str())
            .map_err(|e| HarmonicError::UserId(user.clone(), e))?
            .ok_or(HarmonicError::NoUser(user.clone()))?;
        
        Ok(Self {
            path: path.to_path_buf(),
            user,
            group,
            mode,
        })
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateDirectory {
    type Receipt = CreateDirectoryReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { path, user, group, mode } = &self;
        vec![ActionDescription::new(
            format!("Create the directory `{}`", path.display()),
            vec![format!(
                "Creating directory `{}` owned by `{user}:{group}` with mode `{mode:#o}`", path.display()
            )],
        )]
    }

    async fn execute(self) -> Result<CreateDirectoryReceipt, HarmonicError> {
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

        create_dir(path.clone())
            .await
            .map_err(|e| HarmonicError::CreateDirectory(path.clone(), e))?;
        chown(&path, Some(uid), Some(gid))
            .map_err(|e| HarmonicError::Chown(path.clone(), e))?;
        
        Ok(CreateDirectoryReceipt {
            path,
            user,
            group,
            mode,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateDirectoryReceipt {
    path: PathBuf,
    user: String,
    group: String,
    mode: u32,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateDirectoryReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!("Create the directory `/nix`"),
            vec![format!(
                "Nix and the Nix daemon require a Nix Store, which will be stored at `/nix`"
            )],
        )]
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        let Self {
            path,
            user,
            group,
            mode,
        } = self;

        todo!();

        Ok(())
    }
}
