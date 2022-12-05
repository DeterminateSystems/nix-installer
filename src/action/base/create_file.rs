use nix::unistd::{chown, Group, User};

use std::path::{Path, PathBuf};
use tokio::{
    fs::{remove_file, OpenOptions},
    io::AsyncWriteExt,
};

use crate::action::{Action, ActionDescription, ActionError, StatefulAction};

/** Create a file at the given location with the provided `buf`,
optionally with an owning user, group, and mode.

If `force` is set, the file will always be overwritten (and deleted)
regardless of its presence prior to install.
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateFile {
    pub(crate) path: PathBuf,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
    buf: String,
    force: bool,
}

impl CreateFile {
    #[tracing::instrument(skip_all)]
    pub async fn plan(
        path: impl AsRef<Path>,
        user: impl Into<Option<String>>,
        group: impl Into<Option<String>>,
        mode: impl Into<Option<u32>>,
        buf: String,
        force: bool,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let path = path.as_ref().to_path_buf();

        if path.exists() && !force {
            return Err(ActionError::Exists(path.to_path_buf()));
        }

        Ok(Self {
            path,
            user: user.into(),
            group: group.into(),
            mode: mode.into(),
            buf,
            force,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_file")]
impl Action for CreateFile {
    fn tracing_synopsis(&self) -> String {
        format!("Create or overwrite file `{}`", self.path.display())
    }
    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
        user = self.user,
        group = self.group,
        mode = self.mode.map(|v| tracing::field::display(format!("{:#o}", v))),
    ))]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            user,
            group,
            mode,
            buf,
            force: _,
        } = self;

        let mut options = OpenOptions::new();
        options.create_new(true).write(true).read(true);

        if let Some(mode) = mode {
            options.mode(*mode);
        }

        let mut file = options
            .open(&path)
            .await
            .map_err(|e| ActionError::Open(path.to_owned(), e))?;

        file.write_all(buf.as_bytes())
            .await
            .map_err(|e| ActionError::Write(path.to_owned(), e))?;

        let gid = if let Some(group) = group {
            Some(
                Group::from_name(group.as_str())
                    .map_err(|e| ActionError::GroupId(group.clone(), e))?
                    .ok_or(ActionError::NoGroup(group.clone()))?
                    .gid,
            )
        } else {
            None
        };
        let uid = if let Some(user) = user {
            Some(
                User::from_name(user.as_str())
                    .map_err(|e| ActionError::UserId(user.clone(), e))?
                    .ok_or(ActionError::NoUser(user.clone()))?
                    .uid,
            )
        } else {
            None
        };
        chown(path, uid, gid).map_err(|e| ActionError::Chown(path.clone(), e))?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf: _,
            force: _,
        } = &self;

        vec![ActionDescription::new(
            format!("Delete file `{}`", path.display()),
            vec![format!("Delete file `{}`", path.display())],
        )]
    }

    #[tracing::instrument(skip_all, fields(
        path = %self.path.display(),
        user = self.user,
        group = self.group,
        mode = self.mode.map(|v| tracing::field::display(format!("{:#o}", v))),
    ))]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            path,
            user: _,
            group: _,
            mode: _,
            buf: _,
            force: _,
        } = self;

        remove_file(&path)
            .await
            .map_err(|e| ActionError::Remove(path.to_owned(), e))?;

        Ok(())
    }
}
