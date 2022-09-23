use serde::Serialize;
use tempdir::TempDir;

use crate::actions::base::{FetchNix, FetchNixError, MoveUnpackedNix, MoveUnpackedNixError};
use crate::{HarmonicError, InstallSettings};

use crate::actions::{ActionDescription, Actionable, ActionState, Action};

use super::{
    CreateNixTree, CreateNixTreeError,
    CreateUsersAndGroup, CreateUsersAndGroupError,
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
        let tempdir = TempDir::new("nix").map_err(ProvisionNixError::TempDir)?;

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
impl Actionable for ActionState<ProvisionNix> {
    type Error = ProvisionNixError;
    fn description(&self) -> Vec<ActionDescription> {
        match self {
            ActionState::Completed(action) => action.start_systemd_socket.description(),
            ActionState::Planned(action) => action.start_systemd_socket.description(),
            ActionState::Reverted(_) => todo!(),
        }
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
    async fn execute(&mut self) -> Result<(), Self::Error> {
        let Self {
            fetch_nix,
            create_nix_tree,
            create_users_and_group,
            move_unpacked_nix,
        } = self;

        // We fetch nix while doing the rest, then move it over.
        let fetch_nix_handle = tokio::spawn(async move { fetch_nix.execute().await });

        create_users_and_group.execute().await?;
        create_nix_tree.execute().await?;

        fetch_nix_handle.await??;
        move_unpacked_nix.execute().await?;

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}

impl From<ActionState<ProvisionNix>> for ActionState<Action> {
    fn from(v: ActionState<ProvisionNix>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::ProvisionNix(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::ProvisionNix(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::ProvisionNix(v)),
        }
    }
}


#[derive(Debug, thiserror::Error, Serialize)]
pub enum ProvisionNixError {
    #[error("Failed create tempdir")]
    #[serde(serialize_with = "crate::serialize_std_io_error_to_display")]
    TempDir(#[source] std::io::Error)
}
