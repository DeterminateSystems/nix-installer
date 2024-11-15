use tracing::{span, Span};

use super::CreateNixTree;
use crate::{
    action::{
        base::{FetchAndUnpackNix, MoveUnpackedNix},
        Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
    },
    settings::{CommonSettings, SCRATCH_DIR},
};
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub(crate) const NIX_STORE_LOCATION: &str = "/nix/store";

/**
Place Nix and it's requirements onto the target
 */
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "provision_nix")]
pub struct ProvisionNix {
    nix_store_gid: u32,
    pub(crate) fetch_nix: StatefulAction<FetchAndUnpackNix>,
    pub(crate) create_nix_tree: StatefulAction<CreateNixTree>,
    pub(crate) move_unpacked_nix: StatefulAction<MoveUnpackedNix>,
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
            nix_store_gid: settings.nix_build_group_id,
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
            nix_store_gid: _, // TODO @grahamc ... have a description, I guess
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

        if std::path::Path::new(NIX_STORE_LOCATION).exists() {
            ensure_nix_store_group(self.nix_store_gid)
                .await
                .map_err(Self::error)?;
        }

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
            nix_store_gid: _,
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

/// If there is an existing /nix/store directory, ensure that the group ID we're going to use for
/// the nix build group matches the group that owns /nix/store to prevent weird mismatched-ownership
/// issues.
async fn ensure_nix_store_group(desired_nix_build_group_id: u32) -> Result<(), ActionErrorKind> {
    let previous_store_metadata = tokio::fs::metadata(NIX_STORE_LOCATION)
        .await
        .map_err(|e| ActionErrorKind::GettingMetadata(NIX_STORE_LOCATION.into(), e))?;
    let previous_store_group_id = previous_store_metadata.gid();
    if previous_store_group_id != desired_nix_build_group_id {
        for (entry, metadata) in walkdir::WalkDir::new(NIX_STORE_LOCATION)
        .follow_links(false)
        .same_file_system(true)
        // chown all of the contents of the dir before NIX_STORE_LOCATION,
        // this means our test of "does /nix/store have the right gid?"
        // is useful until the entire store is examined
        .contents_first(true)
        .into_iter()
        .filter_map(|entry| {
            match entry {
                Ok(entry) => Some(entry),
                Err(e) => {
                    tracing::warn!(%e, "Enumerating the Nix store");
                    None
                }
            }
        })
        .filter_map(|entry| match entry.metadata() {
            Ok(metadata) => Some((entry, metadata)),
            Err(e) => {
                tracing::warn!(path = %entry.path().to_string_lossy(), %e, "Reading ownership and mode data");
                None
            }
        })
        .filter(|(_entry, metadata)| {
            if metadata.uid() != 0 {
                return true;
            }

            // If the dirent's group ID is the *previous* GID, reassign.
            // NOTE(@grahamc, 2024-11-15): Nix on macOS has store paths with a group of nixbld, and sometimes a group of `wheel` (0).
            // On NixOS, all the store paths have their GID set to 0.
            // I've enquired into why these are different, but don't have an answer yet.
            if metadata.gid() == previous_store_group_id {
                return true;
            }

            // Get the user-group-other permission bits

            if metadata.mode() != 0o555 || metadata.mode() != 0o444 {
                return true;
            }

            false
        }) {
            if let Err(e) = std::os::unix::fs::chown(entry.path(), Some(0), Some(desired_nix_build_group_id)) {
                tracing::warn!(path = %entry.path().to_string_lossy(), %e, "Failed to set the owner:group to 0:{}", desired_nix_build_group_id);
            }
            let mode = metadata.mode();

            // Determine if this file is executable by anyone
            let is_any_executable = (mode & 0o111) != 0;

            if is_any_executable {
                // If anyone can execute this file, then everyone must be able to
                let all_executable = mode | 0o555;

                if mode != all_executable {
                    if let Err(e) = tokio::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(all_executable)).await {
                        tracing::warn!(path = %entry.path().to_string_lossy(), %e, "Failed to set the mode to 0o555");
                    }
                }
            } else {
                let all_readable = mode | 0o444;

                // Otherwise, if anyone can read the file, then everyone must be able to
                if mode != all_readable {
                    if let Err(e) = tokio::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(all_readable)).await {
                        tracing::warn!(path = %entry.path().to_string_lossy(), %e, "Failed to set the mode to 0o444");
                    }
                }
            }
        }
    }

    Ok(())
}
