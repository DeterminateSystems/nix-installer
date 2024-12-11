//! [`Action`](crate::action::Action)s which only call other base plugins

pub(crate) mod configure_determinate_nixd_init_service;
pub(crate) mod configure_init_service;
pub(crate) mod configure_nix;
pub(crate) mod configure_shell_profile;
pub(crate) mod configure_upstream_init_service;
pub(crate) mod create_nix_tree;
pub(crate) mod create_users_and_groups;
pub(crate) mod delete_users;
pub(crate) mod place_nix_configuration;
pub(crate) mod provision_determinate_nixd;
pub(crate) mod provision_nix;
pub(crate) mod setup_channels;

pub use configure_determinate_nixd_init_service::ConfigureDeterminateNixdInitService;
pub use configure_init_service::{ConfigureInitService, ConfigureNixDaemonServiceError};
pub use configure_nix::ConfigureNix;
pub use configure_shell_profile::ConfigureShellProfile;
pub use configure_upstream_init_service::ConfigureUpstreamInitService;
pub use create_nix_tree::CreateNixTree;
pub use create_users_and_groups::CreateUsersAndGroups;
pub use delete_users::DeleteUsersInGroup;
pub use place_nix_configuration::PlaceNixConfiguration;
pub use provision_determinate_nixd::ProvisionDeterminateNixd;
pub use provision_nix::ProvisionNix;
pub use setup_channels::SetupChannels;
