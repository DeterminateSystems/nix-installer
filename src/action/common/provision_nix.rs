use nix::unistd::Group;
use tracing::{span, Span};

use super::CreateNixTree;
use crate::{
    action::{
        base::{CreateGroup, DeleteUser, FetchAndUnpackNix, MoveUnpackedNix},
        Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
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
    delete_users: Vec<StatefulAction<DeleteUser>>,
    create_group: StatefulAction<CreateGroup>,
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
            settings.ssl_cert_file.clone(),
        )
        .await?;

        let mut delete_users = vec![];
        if let Some(group) = Group::from_name(settings.nix_build_group_name.as_str())
            .map_err(|e| ActionErrorKind::GettingGroupId(settings.nix_build_group_name.clone(), e))
            .map_err(Self::error)?
        {
            if group.gid.as_raw() != settings.nix_build_group_id {
                return Err(Self::error(ActionErrorKind::GroupGidMismatch(
                    settings.nix_build_group_name.clone(),
                    group.gid.as_raw(),
                    settings.nix_build_group_id,
                )));
            }
            for member in group.mem {
                delete_users.push(DeleteUser::plan(member).await?)
            }
        }

        let create_group = CreateGroup::plan(
            settings.nix_build_group_name.clone(),
            settings.nix_build_group_id,
        )
        .map_err(Self::error)?;
        let create_nix_tree = CreateNixTree::plan().await.map_err(Self::error)?;
        let move_unpacked_nix = MoveUnpackedNix::plan(PathBuf::from(SCRATCH_DIR))
            .await
            .map_err(Self::error)?;
        Ok(Self {
            fetch_nix,
            delete_users,
            create_group,
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
            delete_users,
            create_group,
            create_nix_tree,
            move_unpacked_nix,
        } = &self;

        let mut buf = Vec::default();
        buf.append(&mut fetch_nix.describe_execute());

        // TODO: This is a bit... loud
        for delete_user in delete_users {
            buf.append(&mut delete_user.describe_execute());
        }

        buf.append(&mut create_group.describe_execute());
        buf.append(&mut create_nix_tree.describe_execute());
        buf.append(&mut move_unpacked_nix.describe_execute());

        buf
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        // We fetch nix while doing the rest, then move it over.
        let mut fetch_nix_clone = self.fetch_nix.clone();
        let fetch_nix_handle = tokio::task::spawn(async {
            fetch_nix_clone.try_execute().await.map_err(Self::error)?;
            Result::<_, ActionError>::Ok(fetch_nix_clone)
        });

        for delete_users in self.delete_users.iter_mut() {
            delete_users.try_execute().await.map_err(Self::error)?;
        }

        self.create_group.try_execute().await.map_err(Self::error)?;
        self.create_nix_tree
            .try_execute()
            .await
            .map_err(Self::error)?;

        self.fetch_nix = fetch_nix_handle
            .await
            .map_err(ActionErrorKind::Join)
            .map_err(Self::error)??;
        self.move_unpacked_nix
            .try_execute()
            .await
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            fetch_nix,
            delete_users,
            create_group,
            create_nix_tree,
            move_unpacked_nix,
        } = &self;

        let mut buf = Vec::default();
        buf.append(&mut move_unpacked_nix.describe_revert());
        buf.append(&mut create_nix_tree.describe_revert());
        buf.append(&mut create_group.describe_revert());
        for delete_user in delete_users {
            buf.append(&mut delete_user.describe_revert());
        }
        buf.append(&mut fetch_nix.describe_revert());
        buf
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];

        if let Err(err) = self.fetch_nix.try_revert().await {
            errors.push(err)
        }

        if let Err(err) = self.create_group.try_revert().await {
            errors.push(err)
        }

        if let Err(err) = self.create_group.try_revert().await {
            errors.push(err)
        }
        if let Err(err) = self.create_nix_tree.try_revert().await {
            errors.push(err)
        }

        if let Err(err) = self.move_unpacked_nix.try_revert().await {
            errors.push(err)
        }

        if errors.is_empty() {
            Ok(())
        } else if errors.len() == 1 {
            Err(errors
                .into_iter()
                .next()
                .expect("Expected 1 len Vec to have at least 1 item"))
        } else {
            Err(Self::error(ActionErrorKind::MultipleChildren(errors)))
        }
    }
}
