use tracing::{span, Span};

use super::{CreateNixTree, CreateUsersAndGroups};
use crate::{
    action::{
        base::{FetchAndUnpackNix, MoveUnpackedNix},
        Action, ActionDescription, ActionError, ActionTag, StatefulAction,
    },
    settings::{CommonSettings, SCRATCH_DIR},
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
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(settings: &CommonSettings) -> Result<StatefulAction<Self>, ActionError> {
        let fetch_nix = FetchAndUnpackNix::plan(
            settings.nix_package_url.clone(),
            PathBuf::from(SCRATCH_DIR),
            settings.proxy.clone(),
        )
        .await?;
        let create_users_and_group = CreateUsersAndGroups::plan(settings.clone())
            .await
            .map_err(|e| ActionError::Child(CreateUsersAndGroups::action_tag(), Box::new(e)))?;
        let create_nix_tree = CreateNixTree::plan()
            .await
            .map_err(|e| ActionError::Child(CreateNixTree::action_tag(), Box::new(e)))?;
        let move_unpacked_nix = MoveUnpackedNix::plan(PathBuf::from(SCRATCH_DIR))
            .await
            .map_err(|e| ActionError::Child(MoveUnpackedNix::action_tag(), Box::new(e)))?;
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
    fn action_tag() -> ActionTag {
        ActionTag("provision_nix")
    }
    fn tracing_synopsis(&self) -> String {
        "Provision Nix".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "provision_nix",)
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

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // We fetch nix while doing the rest, then move it over.
        let mut fetch_nix_clone = self.fetch_nix.clone();
        let fetch_nix_handle = tokio::task::spawn(async {
            fetch_nix_clone
                .try_execute()
                .await
                .map_err(|e| ActionError::Child(fetch_nix_clone.action_tag(), Box::new(e)))?;
            Result::<_, ActionError>::Ok(fetch_nix_clone)
        });

        self.create_users_and_group
            .try_execute()
            .await
            .map_err(|e| {
                ActionError::Child(self.create_users_and_group.action_tag(), Box::new(e))
            })?;
        self.create_nix_tree
            .try_execute()
            .await
            .map_err(|e| ActionError::Child(self.create_nix_tree.action_tag(), Box::new(e)))?;

        self.fetch_nix = fetch_nix_handle.await.map_err(ActionError::Join)??;
        self.move_unpacked_nix
            .try_execute()
            .await
            .map_err(|e| ActionError::Child(self.move_unpacked_nix.action_tag(), Box::new(e)))?;

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

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        // We fetch nix while doing the rest, then move it over.
        let mut fetch_nix_clone = self.fetch_nix.clone();
        let fetch_nix_handle = tokio::task::spawn(async {
            fetch_nix_clone
                .try_revert()
                .await
                .map_err(|e| ActionError::Child(fetch_nix_clone.action_tag(), Box::new(e)))?;
            Result::<_, ActionError>::Ok(fetch_nix_clone)
        });

        if let Err(err) = self.create_users_and_group.try_revert().await {
            fetch_nix_handle.abort();
            return Err(err);
        }
        if let Err(err) = self.create_nix_tree.try_revert().await {
            fetch_nix_handle.abort();
            return Err(err);
        }

        self.fetch_nix = fetch_nix_handle
            .await
            .map_err(ActionError::Join)?
            .map_err(|e| ActionError::Child(self.fetch_nix.action_tag(), Box::new(e)))?;
        self.move_unpacked_nix
            .try_revert()
            .await
            .map_err(|e| ActionError::Child(self.move_unpacked_nix.action_tag(), Box::new(e)))?;

        Ok(())
    }
}
