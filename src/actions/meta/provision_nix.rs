use serde::{Deserialize, Serialize};
use tempdir::TempDir;

use crate::actions::base::{FetchNix, FetchNixReceipt, MoveUnpackedNix, MoveUnpackedNixReceipt};
use crate::error::ActionState;
use crate::{HarmonicError, InstallSettings};

use crate::actions::{ActionDescription, Actionable, Revertable};

use super::{
    CreateNixTree, CreateNixTreeReceipt,
    CreateUsersAndGroup, CreateUsersAndGroupReceipt,
};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ProvisionNix {
    fetch_nix: FetchNix,
    create_users_and_group: CreateUsersAndGroup,
    create_nix_tree: CreateNixTree,
    move_unpacked_nix: MoveUnpackedNix,
}

impl ProvisionNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(settings: InstallSettings) -> Result<ActionState<Self>, ProvisionNixError> {
        let tempdir = TempDir::new("nix").map_err(HarmonicError::TempDir)?;

        let fetch_nix = FetchNix::plan(
            settings.nix_package_url.clone(),
            tempdir.path().to_path_buf(),
        )
        .await?;

        let create_users_and_group = CreateUsersAndGroup::plan(settings.clone()).await?;
        let create_nix_tree = CreateNixTree::plan(settings.force).await?;
        let move_unpacked_nix = MoveUnpackedNix::plan(tempdir.path().to_path_buf()).await?;

        Ok(ActionState::Planned(Self {
            fetch_nix,
            create_users_and_group,
            create_nix_tree,
            move_unpacked_nix,
        }))
    }
}

#[async_trait::async_trait]
impl Actionable for ProvisionNix {
    type Receipt = ProvisionNixReceipt;
    type Error = ProvisionNixError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            fetch_nix,
            create_users_and_group,
            create_nix_tree,
            move_unpacked_nix,
        } = &self;

        let mut buf = fetch_nix.description();
        buf.append(&mut create_users_and_group.description());
        buf.append(&mut create_nix_tree.description());
        buf.append(&mut move_unpacked_nix.description());

        buf
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> ActionState<Self> {
        let Self {
            fetch_nix,
            create_nix_tree,
            create_users_and_group,
            move_unpacked_nix,
        } = self;

        // We fetch nix while doing the rest, then move it over.
        let fetch_nix_handle = tokio::spawn(async move { fetch_nix.execute().await });

        let create_users_and_group = create_users_and_group.execute().await;
        if create_users_and_group.errored() {

        }

        let create_nix_tree = create_nix_tree.execute().await;

        let fetch_nix = fetch_nix_handle.await;
        let move_unpacked_nix = move_unpacked_nix.execute().await;

        ActionState::Attempted(ProvisionNixReceipt {
            fetch_nix,
            create_users_and_group,
            create_nix_tree,
            move_unpacked_nix,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ProvisionNixReceipt {
    fetch_nix: ActionState<FetchNix>,
    create_users_and_group: ActionState<CreateUsersAndGroup>,
    create_nix_tree: ActionState<CreateNixTree>,
    move_unpacked_nix: ActionState<MoveUnpackedNix>,
}

#[async_trait::async_trait]
impl Revertable for ProvisionNixReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}

#[derive(thiserror::Error, Debug, Serialize, Deserialize)]
pub enum ProvisionNixError {

}