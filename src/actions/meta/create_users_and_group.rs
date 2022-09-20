use tokio::task::JoinSet;

use crate::{HarmonicError, InstallSettings};

use crate::actions::base::{CreateGroup, CreateUserReceipt, CreateGroupReceipt};
use crate::actions::{ActionDescription, ActionReceipt, Actionable, CreateUser, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUsersAndGroup {
    settings: InstallSettings,
    create_group: CreateGroup,
    create_users: Vec<CreateUser>,
}

impl CreateUsersAndGroup {
    pub fn plan(
        settings: InstallSettings
    ) -> Self {
        let create_group = CreateGroup::plan(settings.nix_build_group_name.clone(), settings.nix_build_group_id);
        let create_users = (0..settings.daemon_user_count)
            .map(|count| {
                CreateUser::plan(
                    format!("{}{count}", settings.nix_build_user_prefix),
                    settings.nix_build_user_id_base + count,
                )
            })
            .collect();
        Self {
            settings,
            create_group,
            create_users,
        }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateUsersAndGroup {
    type Receipt = CreateUsersAndGroupReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            create_users: _,
            create_group: _,
            settings: InstallSettings {
                explain: _,
                daemon_user_count,
                channels: _,
                modify_profile: _,
                nix_build_group_name,
                nix_build_group_id,
                nix_build_user_prefix,
                nix_build_user_id_base,
                nix_package_url,
            }
        } = &self;

        vec![
            ActionDescription::new(
                format!("Create build users and group"),
                vec![
                    format!("The nix daemon requires system users it can act as in order to build"),
                    format!("This action will create group `{nix_build_group_name}` with uid `{nix_build_group_id}`"),
                    format!("This action will create {daemon_user_count} users with prefix `{nix_build_user_prefix}` starting at uid `{nix_build_user_id_base}`"),
                ],
            )
        ]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { create_users, create_group, settings: _ } = self;

        // Create group
        let create_group = create_group.execute().await?;

        // Create users
        // TODO(@hoverbear): Abstract this, it will be common
        let mut set = JoinSet::new();
        let mut successes = Vec::with_capacity(create_users.len());
        let mut errors = Vec::default();

        for create_user in create_users {
            let _abort_handle = set.spawn(async move { create_user.execute().await });
        }

        while let Some(result) = set.join_next().await {
            match result {
                Ok(Ok(success)) => successes.push(success),
                Ok(Err(e)) => errors.push(e),
                Err(e) => errors.push(e.into()),
            };
        }

        if !errors.is_empty() {
            // If we got an error in a child, we need to revert the successful ones:
            let mut failed_reverts = Vec::default();
            for success in successes {
                match success.revert().await {
                    Ok(()) => (),
                    Err(e) => failed_reverts.push(e),
                }
            }

            if !failed_reverts.is_empty() {
                return Err(HarmonicError::FailedReverts(errors, failed_reverts));
            }

            if errors.len() == 1 {
                return Err(errors.into_iter().next().unwrap());
            } else {
                return Err(HarmonicError::Multiple(errors));
            }
        }

        Ok(CreateUsersAndGroupReceipt {
            create_group,
            create_users: successes,
        })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUsersAndGroupReceipt {
    create_group: CreateGroupReceipt,
    create_users: Vec<CreateUserReceipt>,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateUsersAndGroupReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
