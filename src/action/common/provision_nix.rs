use tracing::{span, Span};

use super::CreateNixTree;
use crate::{
    action::{
        base::{FetchAndUnpackNix, MoveUnpackedNix},
        Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
    },
    settings::{CommonSettings, SCRATCH_DIR},
};
use std::path::PathBuf;

/**
Place Nix and it's requirements onto the target
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "provision_nix")]
pub struct ProvisionNix {
    fetch_nix: StatefulAction<FetchAndUnpackNix>,
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

        let create_nix_tree = CreateNixTree::plan().await.map_err(Self::error)?;
        let move_unpacked_nix = MoveUnpackedNix::plan(PathBuf::from(SCRATCH_DIR))
            .await
            .map_err(Self::error)?;
        Ok(Self {
            fetch_nix,
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
            create_nix_tree,
            move_unpacked_nix,
        } = &self;

        let mut buf = Vec::default();
        buf.append(&mut fetch_nix.describe_execute());

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
            create_nix_tree,
            move_unpacked_nix,
        } = &self;

        let mut buf = Vec::default();
        buf.append(&mut move_unpacked_nix.describe_revert());
        buf.append(&mut create_nix_tree.describe_revert());

        buf.append(&mut fetch_nix.describe_revert());
        buf
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];

        if let Err(err) = self.fetch_nix.try_revert().await {
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
