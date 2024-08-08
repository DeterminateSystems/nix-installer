use crate::{
    action::{
        base::{AddUserToGroup, CreateGroup, CreateUser},
        Action, ActionDescription, ActionError, ActionErrorKind, ActionTag, StatefulAction,
    },
    settings::CommonSettings,
};
use tracing::{span, Span};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "create_users_and_group")]
pub struct CreateUsersAndGroups {
    nix_build_user_count: u32,
    nix_build_group_name: String,
    nix_build_group_id: u32,
    nix_build_user_prefix: String,
    nix_build_user_id_base: u32,
    create_group: StatefulAction<CreateGroup>,
    create_users: Vec<StatefulAction<CreateUser>>,
    add_users_to_groups: Vec<StatefulAction<AddUserToGroup>>,
}

impl CreateUsersAndGroups {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(settings: CommonSettings) -> Result<StatefulAction<Self>, ActionError> {
        let create_group = CreateGroup::plan(
            settings.nix_build_group_name.clone(),
            settings.nix_build_group_id,
        )?;
        let mut create_users = Vec::with_capacity(settings.nix_build_user_count as usize);
        let mut add_users_to_groups = Vec::with_capacity(settings.nix_build_user_count as usize);
        for index in 1..=settings.nix_build_user_count {
            create_users.push(
                CreateUser::plan(
                    format!("{}{index}", settings.nix_build_user_prefix),
                    settings.nix_build_user_id_base + index,
                    settings.nix_build_group_name.clone(),
                    settings.nix_build_group_id,
                    format!("Nix build user {index}"),
                )
                .await
                .map_err(Self::error)?,
            );
            add_users_to_groups.push(
                AddUserToGroup::plan(
                    format!("{}{index}", settings.nix_build_user_prefix),
                    settings.nix_build_user_id_base + index,
                    settings.nix_build_group_name.clone(),
                    settings.nix_build_group_id,
                )
                .await
                .map_err(Self::error)?,
            );
        }
        Ok(Self {
            nix_build_user_count: settings.nix_build_user_count,
            nix_build_group_name: settings.nix_build_group_name,
            nix_build_group_id: settings.nix_build_group_id,
            nix_build_user_prefix: settings.nix_build_user_prefix,
            nix_build_user_id_base: settings.nix_build_user_id_base,
            create_group,
            create_users,
            add_users_to_groups,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "create_users_and_group")]
impl Action for CreateUsersAndGroups {
    fn action_tag() -> ActionTag {
        ActionTag("create_users_and_group")
    }
    fn tracing_synopsis(&self) -> String {
        if self.create_users.is_empty() {
            format!("Create build group (GID {})", self.nix_build_group_id)
        } else {
            format!(
                "Create build users (UID {}-{}) and group (GID {})",
                self.nix_build_user_id_base + 1,
                self.nix_build_user_id_base + self.nix_build_user_count,
                self.nix_build_group_id
            )
        }
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "create_users_and_group",
            nix_build_user_count = self.nix_build_user_count,
            nix_build_group_name = self.nix_build_group_name,
            nix_build_group_id = self.nix_build_group_id,
            nix_build_user_prefix = self.nix_build_user_prefix,
            nix_build_user_id_base = self.nix_build_user_id_base,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let Self {
            nix_build_user_count: _,
            nix_build_group_name: _,
            nix_build_group_id: _,
            nix_build_user_prefix: _,
            nix_build_user_id_base: _,
            create_group,
            create_users,
            add_users_to_groups,
        } = &self;

        let mut create_users_descriptions = Vec::new();
        for create_user in create_users {
            if let Some(val) = create_user.describe_execute().first() {
                create_users_descriptions.push(val.description.clone())
            }
        }

        let mut add_user_to_group_descriptions = Vec::new();
        for add_user_to_group in add_users_to_groups {
            if let Some(val) = add_user_to_group.describe_execute().first() {
                add_user_to_group_descriptions.push(val.description.clone())
            }
        }

        let mut explanation = vec![
            format!("The Nix daemon requires system users (and a group they share) which it can act as in order to build"),
        ];
        if let Some(val) = create_group.describe_execute().first() {
            explanation.push(val.description.clone())
        }
        explanation.append(&mut create_users_descriptions);
        explanation.append(&mut add_user_to_group_descriptions);

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        let Self {
            create_users,
            create_group,
            add_users_to_groups,
            nix_build_user_count: _,
            nix_build_group_name: _,
            nix_build_group_id: _,
            nix_build_user_prefix: _,
            nix_build_user_id_base: _,
        } = self;

        // Create group
        create_group.try_execute().await?;

        // Mac is apparently not threadsafe here...
        use target_lexicon::OperatingSystem;
        match OperatingSystem::host() {
            OperatingSystem::MacOSX {
                major: _,
                minor: _,
                patch: _,
            }
            | OperatingSystem::Darwin => {
                for create_user in create_users.iter_mut() {
                    create_user.try_execute().await.map_err(Self::error)?;
                }
            },
            _ => {
                for create_user in create_users.iter_mut() {
                    create_user.try_execute().await.map_err(Self::error)?;
                }
                // While we may be tempted to do something like this, it can break on many older OSes like Ubuntu 18.04:
                // ```
                // useradd: cannot lock /etc/passwd; try again later.
                // ```
                // So, instead, we keep this here in hopes one day we can enable it for some detected OS:
                //
                // let mut set = JoinSet::new();
                // let mut errors: Vec<Box<ActionError>> = Vec::new();
                // for (idx, create_user) in create_users.iter_mut().enumerate() {
                //     let span = tracing::Span::current().clone();
                //     let mut create_user_clone = create_user.clone();
                //     let _abort_handle = set.spawn(async move {
                //         create_user_clone.try_execute().instrument(span).await?;
                //         Result::<_, _>::Ok((idx, create_user_clone))
                //     });
                // }

                // while let Some(result) = set.join_next().await {
                //     match result {
                //         Ok(Ok((idx, success))) => create_users[idx] = success,
                //         Ok(Err(e)) => errors.push(Box::new(e)),
                //         Err(e) => return Err(ActionErrorKind::Join(e))?,
                //     };
                // }

                // if !errors.is_empty() {
                //     if errors.len() == 1 {
                //         return Err(errors.into_iter().next().unwrap().into());
                //     } else {
                //         return Err(ActionErrorKind::Children(errors));
                //     }
                // }
            },
        };

        for add_user_to_group in add_users_to_groups.iter_mut() {
            add_user_to_group.try_execute().await.map_err(Self::error)?;
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let Self {
            nix_build_user_count: _,
            nix_build_group_name: _,
            nix_build_group_id: _,
            nix_build_user_prefix: _,
            nix_build_user_id_base: _,
            create_group,
            create_users,
            add_users_to_groups,
        } = &self;
        let mut create_users_descriptions = Vec::new();
        for create_user in create_users {
            if let Some(val) = create_user.describe_revert().first() {
                create_users_descriptions.push(val.description.clone())
            }
        }

        let mut add_user_to_group_descriptions = Vec::new();
        for add_user_to_group in add_users_to_groups {
            if let Some(val) = add_user_to_group.describe_revert().first() {
                add_user_to_group_descriptions.push(val.description.clone())
            }
        }

        let mut explanation = vec![
            format!("The Nix daemon requires system users (and a group they share) which it can act as in order to build"),
        ];
        if let Some(val) = create_group.describe_revert().first() {
            explanation.push(val.description.clone())
        }
        explanation.append(&mut create_users_descriptions);
        explanation.append(&mut add_user_to_group_descriptions);

        if create_users.is_empty() {
            vec![ActionDescription::new(
                "Remove Nix group".to_string(),
                explanation,
            )]
        } else {
            vec![ActionDescription::new(
                "Remove Nix users and group".to_string(),
                explanation,
            )]
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];
        for create_user in self.create_users.iter_mut() {
            if let Err(err) = create_user.try_revert().await {
                errors.push(err);
            }
        }

        // We don't actually need to do this, when a user is deleted they are removed from groups
        // for add_user_to_group in add_users_to_groups.iter_mut() {
        //     add_user_to_group.try_revert().await?;
        // }

        // Create group
        if let Err(err) = self.create_group.try_revert().await {
            errors.push(err);
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
