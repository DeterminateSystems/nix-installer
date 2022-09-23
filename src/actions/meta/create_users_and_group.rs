use serde::Serialize;
use tokio::task::JoinSet;

use crate::{HarmonicError, InstallSettings};

use crate::actions::base::{CreateGroup, CreateGroupError, CreateUserError};
use crate::actions::{ActionDescription, Actionable, CreateUser, ActionState, Action};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUsersAndGroup {
    daemon_user_count: usize,
    nix_build_group_name: String,
    nix_build_group_id: usize,
    nix_build_user_prefix: String,
    nix_build_user_id_base: usize,
    create_group: CreateGroup,
    create_users: Vec<CreateUser>,
}

impl CreateUsersAndGroup {
    #[tracing::instrument(skip_all)]
    pub async fn plan(settings: InstallSettings) -> Result<Self, HarmonicError> {
        // TODO(@hoverbear): CHeck if it exist, error if so
        let create_group = CreateGroup::plan(
            settings.nix_build_group_name.clone(),
            settings.nix_build_group_id,
        );
        // TODO(@hoverbear): CHeck if they exist, error if so
        let create_users = (0..settings.daemon_user_count)
            .map(|count| {
                CreateUser::plan(
                    format!("{}{count}", settings.nix_build_user_prefix),
                    settings.nix_build_user_id_base + count,
                    settings.nix_build_group_id,
                )
            })
            .collect();
        Ok(Self {
            daemon_user_count: settings.daemon_user_count,
            nix_build_group_name: settings.nix_build_group_name,
            nix_build_group_id: settings.nix_build_group_id,
            nix_build_user_prefix: settings.nix_build_user_prefix,
            nix_build_user_id_base: settings.nix_build_user_id_base,
            create_group,
            create_users,
        })
    }
}

#[async_trait::async_trait]
impl Actionable for ActionState<CreateUsersAndGroup> {
    type Error = CreateUsersAndGroupError;
    fn description(&self) -> Vec<ActionDescription> {
        let Self {
            daemon_user_count,
            nix_build_group_name,
            nix_build_group_id,
            nix_build_user_prefix,
            nix_build_user_id_base,
            ..
        } = &self;

        vec![
            ActionDescription::new(
                format!("Create build users and group"),
                vec![
                    format!("The nix daemon requires system users (and a group they share) which it can act as in order to build"),
                    format!("Create group `{nix_build_group_name}` with uid `{nix_build_group_id}`"),
                    format!("Create {daemon_user_count} users with prefix `{nix_build_user_prefix}` starting at uid `{nix_build_user_id_base}`"),
                ],
            )
        ]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(&mut self) -> Result<(), HarmonicError> {
        let Self {
            create_users,
            create_group,
            ..
        } = self;

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
            if errors.len() == 1 {
                return Err(errors.into_iter().next().unwrap());
            } else {
                return Err(HarmonicError::Multiple(errors));
            }
        }

        Ok(Self::Receipt {
            create_group,
            create_users: successes,
        })
    }

    #[tracing::instrument(skip_all)]
    async fn revert(&mut self) -> Result<(), Self::Error> {
        todo!();

        Ok(())
    }
}


impl From<ActionState<CreateUsersAndGroup>> for ActionState<Action> {
    fn from(v: ActionState<CreateUsersAndGroup>) -> Self {
        match v {
            ActionState::Completed(_) => ActionState::Completed(Action::CreateUsersAndGroup(v)),
            ActionState::Planned(_) => ActionState::Planned(Action::CreateUsersAndGroup(v)),
            ActionState::Reverted(_) => ActionState::Reverted(Action::CreateUsersAndGroup(v)),
        }
    }
}


#[derive(Debug, thiserror::Error, Serialize)]
pub enum CreateUsersAndGroupError {

}

