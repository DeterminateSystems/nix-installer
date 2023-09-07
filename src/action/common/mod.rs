//! [`Action`](crate::action::Action)s which only call other base plugins

pub(crate) mod configure_init_service;
pub(crate) mod configure_nix;
pub(crate) mod configure_shell_profile;
pub(crate) mod create_nix_tree;
pub(crate) mod create_users_and_groups;
pub(crate) mod delete_users_in_group;
pub(crate) mod place_nix_configuration;
pub(crate) mod provision_nix;

pub use configure_init_service::{ConfigureInitService, ConfigureNixDaemonServiceError};
pub use configure_nix::ConfigureNix;
pub use configure_shell_profile::ConfigureShellProfile;
pub use create_nix_tree::CreateNixTree;
pub use create_users_and_groups::CreateUsersAndGroups;
pub use delete_users_in_group::DeleteUsersInGroup;
pub use place_nix_configuration::PlaceNixConfiguration;
pub use provision_nix::ProvisionNix;

use super::KnownAction;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub enum CommonAction {
    ConfigureInitService(ConfigureInitService),
    ConfigureNix(ConfigureNix),
    ConfigureShellProfile(ConfigureShellProfile),
    CreateNixTree(CreateNixTree),
    CreateUsersAndGroups(CreateUsersAndGroups),
    DeleteUsersInGroup(DeleteUsersInGroup),
    PlaceNixConfiguration(PlaceNixConfiguration),
    ProvisionNix(ProvisionNix),
}

impl Into<KnownAction> for CommonAction {
    fn into(self) -> KnownAction {
        KnownAction::Common(self)
    }
}