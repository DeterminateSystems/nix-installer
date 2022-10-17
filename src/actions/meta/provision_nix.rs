use std::path::PathBuf;

use serde::Serialize;
use tokio::task::JoinError;

use crate::actions::base::{
    CreateDirectory, CreateDirectoryError, FetchNix, FetchNixError, MoveUnpackedNix,
    MoveUnpackedNixError,
};
use crate::InstallSettings;

use crate::actions::{Action, ActionDescription, ActionState, Actionable};

use super::{CreateNixTree, CreateNixTreeError, CreateUsersAndGroup, CreateUsersAndGroupError};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ProvisionNix {
    create_nix_dir: CreateDirectory,
    fetch_nix: FetchNix,
    create_users_and_group: CreateUsersAndGroup,
    create_nix_tree: CreateNixTree,
    move_unpacked_nix: MoveUnpackedNix,
    action_state: ActionState,
}

impl ProvisionNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(settings: InstallSettings) -> Result<Self, ProvisionNixError> {
        let create_nix_dir =
            CreateDirectory::plan("/nix", "root".into(), "root".into(), 0o0755, true).await?;

        let fetch_nix = FetchNix::plan(
            settings.nix_package_url.clone(),
            PathBuf::from("/nix/temp-install-dir"),
        )
        .await?;
        let create_users_and_group = CreateUsersAndGroup::plan(settings.clone()).await?;
        let create_nix_tree = CreateNixTree::plan().await?;
        let move_unpacked_nix =
            MoveUnpackedNix::plan(PathBuf::from("/nix/temp-install-dir")).await?;
        Ok(Self {
            create_nix_dir,
            fetch_nix,
            create_users_and_group,
            create_nix_tree,
            move_unpacked_nix,
            action_state: ActionState::Uncompleted,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ProvisionNix {
    type Error = ProvisionNixError;
    fn describe_execute(&self) -> Vec<ActionDescription> {
        let Self {
            create_nix_dir,
            fetch_nix,
            create_users_and_group,
            create_nix_tree,
            move_unpacked_nix,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Completed {
            vec![]
        } else {
            let mut buf = Vec::default();
            buf.append(&mut create_nix_dir.describe_execute());
            buf.append(&mut fetch_nix.describe_execute());
            buf.append(&mut create_users_and_group.describe_execute());
            buf.append(&mut create_nix_tree.describe_execute());
            buf.append(&mut move_unpacked_nix.describe_execute());

            buf
        }
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            create_nix_dir,
            fetch_nix,
            create_nix_tree,
            create_users_and_group,
            move_unpacked_nix,
            action_state,
        } = self;
        if *action_state == ActionState::Completed {
            tracing::trace!("Already completed: Provisioning Nix");
            return Ok(());
        }
        *action_state = ActionState::Progress;
        tracing::debug!("Provisioning Nix");

        create_nix_dir.execute().await?;

        // We fetch nix while doing the rest, then move it over.
        let mut fetch_nix_clone = fetch_nix.clone();
        let fetch_nix_handle = tokio::task::spawn(async {
            fetch_nix_clone.execute().await?;
            Result::<_, Self::Error>::Ok(fetch_nix_clone)
        });

        create_users_and_group.execute().await?;
        create_nix_tree
            .execute()
            .await
            .map_err(ProvisionNixError::from)?;

        *fetch_nix = fetch_nix_handle.await.map_err(ProvisionNixError::from)??;
        move_unpacked_nix.execute().await?;

        tracing::trace!("Provisioned Nix");
        *action_state = ActionState::Completed;
        Ok(())
    }

    fn describe_revert(&self) -> Vec<ActionDescription> {
        let Self {
            create_nix_dir,
            fetch_nix,
            create_users_and_group,
            create_nix_tree,
            move_unpacked_nix,
            action_state: _,
        } = &self;
        if self.action_state == ActionState::Uncompleted {
            vec![]
        } else {
            let mut buf = Vec::default();
            buf.append(&mut move_unpacked_nix.describe_revert());
            buf.append(&mut create_nix_tree.describe_revert());
            buf.append(&mut create_users_and_group.describe_revert());
            buf.append(&mut fetch_nix.describe_revert());
            buf.append(&mut create_nix_dir.describe_revert());
            buf
        }
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        let Self {
            create_nix_dir,
            fetch_nix,
            create_nix_tree,
            create_users_and_group,
            move_unpacked_nix,
            action_state,
        } = self;
        if *action_state == ActionState::Uncompleted {
            tracing::trace!("Already reverted: Unprovisioning nix");
            return Ok(());
        }
        *action_state = ActionState::Progress;
        tracing::debug!("Unprovisioning nix");

        // We fetch nix while doing the rest, then move it over.
        let mut fetch_nix_clone = fetch_nix.clone();
        let fetch_nix_handle = tokio::task::spawn(async {
            fetch_nix_clone.revert().await?;
            Result::<_, Self::Error>::Ok(fetch_nix_clone)
        });

        if let Err(err) = create_users_and_group.revert().await {
            fetch_nix_handle.abort();
            return Err(Self::Error::from(err));
        }
        if let Err(err) = create_nix_tree.revert().await {
            fetch_nix_handle.abort();
            return Err(Self::Error::from(err));
        }

        *fetch_nix = fetch_nix_handle.await.map_err(ProvisionNixError::from)??;
        move_unpacked_nix.revert().await?;

        create_nix_dir.revert().await?;

        tracing::trace!("Unprovisioned Nix");
        *action_state = ActionState::Uncompleted;
        Ok(())
    }
}

impl From<ProvisionNix> for Action {
    fn from(v: ProvisionNix) -> Self {
        Action::ProvisionNix(v)
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum ProvisionNixError {
    #[error("Fetching Nix")]
    FetchNix(
        #[source]
        #[from]
        FetchNixError,
    ),
    #[error("Joining spawned async task")]
    Join(
        #[source]
        #[from]
        #[serde(serialize_with = "crate::serialize_error_to_display")]
        JoinError,
    ),
    #[error("Creating directory")]
    CreateDirectory(
        #[source]
        #[from]
        CreateDirectoryError,
    ),
    #[error("Creating users and group")]
    CreateUsersAndGroup(
        #[source]
        #[from]
        CreateUsersAndGroupError,
    ),
    #[error("Creating nix tree")]
    CreateNixTree(
        #[source]
        #[from]
        CreateNixTreeError,
    ),
    #[error("Moving unpacked nix")]
    MoveUnpackedNix(
        #[source]
        #[from]
        MoveUnpackedNixError,
    ),
}
