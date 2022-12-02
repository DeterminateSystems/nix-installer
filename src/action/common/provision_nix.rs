use super::{CreateNixTree, CreateUsersAndGroups};
use crate::{
    action::{
        base::{FetchAndUnpackNix, MoveUnpackedNix},
        Action, ActionDescription, ActionError, StatefulAction,
    },
    settings::CommonSettings,
};
use std::path::PathBuf;

/**
Place Nix and it's requirements onto the target
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ProvisionNix {
    fetch_nix: StatefulAction<FetchAndUnpackNix>,
    create_users_and_group: StatefulAction<CreateUsersAndGroups>,
    create_nix_tree: StatefulAction<CreateNixTree>,
    move_unpacked_nix: StatefulAction<MoveUnpackedNix>,
}

impl ProvisionNix {
    #[tracing::instrument(skip_all)]
    pub async fn plan(settings: &CommonSettings) -> Result<StatefulAction<Self>, ActionError> {
        let fetch_nix = FetchAndUnpackNix::plan(
            settings.nix_package_url.clone(),
            PathBuf::from("/nix/temp-install-dir"),
        )
        .await?;
        let create_users_and_group = CreateUsersAndGroups::plan(settings.clone()).await?;
        let create_nix_tree = CreateNixTree::plan().await?;
        let move_unpacked_nix =
            MoveUnpackedNix::plan(PathBuf::from("/nix/temp-install-dir")).await?;
        Ok(Self {
            fetch_nix,
            create_users_and_group,
            create_nix_tree,
            move_unpacked_nix,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "provision_nix")]
impl Action for ProvisionNix {
    fn tracing_synopsis(&self) -> String {
        "Provision Nix".to_string()
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let Self {
            fetch_nix,
            create_users_and_group,
            create_nix_tree,
            move_unpacked_nix,
        } = &self;

        let mut buf = Vec::default();
        buf.append(&mut fetch_nix.describe_execute());
        buf.append(&mut create_users_and_group.describe_execute());
        buf.append(&mut create_nix_tree.describe_execute());
        buf.append(&mut move_unpacked_nix.describe_execute());

        buf
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            fetch_nix,
            create_nix_tree,
            create_users_and_group,
            move_unpacked_nix,
        } = self;

        // We fetch nix while doing the rest, then move it over.
        let mut fetch_nix_clone = fetch_nix.clone();
        let fetch_nix_handle = tokio::task::spawn(async {
            fetch_nix_clone.try_execute().await?;
            Result::<_, ActionError>::Ok(fetch_nix_clone)
        });

        create_users_and_group.try_execute().await?;
        create_nix_tree.try_execute().await?;

        *fetch_nix = fetch_nix_handle.await.map_err(ActionError::Join)??;
        move_unpacked_nix.try_execute().await?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            fetch_nix,
            create_users_and_group,
            create_nix_tree,
            move_unpacked_nix,
        } = &self;

        let mut buf = Vec::default();
        buf.append(&mut move_unpacked_nix.describe_revert());
        buf.append(&mut create_nix_tree.describe_revert());
        buf.append(&mut create_users_and_group.describe_revert());
        buf.append(&mut fetch_nix.describe_revert());
        buf
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let Self {
            fetch_nix,
            create_nix_tree,
            create_users_and_group,
            move_unpacked_nix,
        } = self;

        // We fetch nix while doing the rest, then move it over.
        let mut fetch_nix_clone = fetch_nix.clone();
        let fetch_nix_handle = tokio::task::spawn(async {
            fetch_nix_clone.try_revert().await?;
            Result::<_, ActionError>::Ok(fetch_nix_clone)
        });

        if let Err(err) = create_users_and_group.try_revert().await {
            fetch_nix_handle.abort();
            return Err(err);
        }
        if let Err(err) = create_nix_tree.try_revert().await {
            fetch_nix_handle.abort();
            return Err(err);
        }

        *fetch_nix = fetch_nix_handle.await.map_err(ActionError::Join)??;
        move_unpacked_nix.try_revert().await?;

        Ok(())
    }
}
