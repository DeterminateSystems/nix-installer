use crate::action::{
    base::DeleteUser, Action, ActionDescription, ActionError, ActionErrorKind, ActionTag,
    StatefulAction,
};
use tracing::{span, Span};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "delete_users_in_group")]
pub struct DeleteUsersInGroup {
    group_name: String,
    group_id: u32,
    delete_users: Vec<StatefulAction<DeleteUser>>,
}

impl DeleteUsersInGroup {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(
        group_name: String,
        group_id: u32,
        users: Vec<String>,
    ) -> Result<StatefulAction<Self>, ActionError> {
        let mut delete_users = vec![];
        for users in users {
            delete_users.push(DeleteUser::plan(users).await?)
        }

        Ok(Self {
            group_name,
            group_id,
            delete_users,
        }
        .into())
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "delete_users_in_group")]
impl Action for DeleteUsersInGroup {
    fn action_tag() -> ActionTag {
        ActionTag("delete_users_in_group")
    }
    fn tracing_synopsis(&self) -> String {
        format!(
            "Delete users part of group `{}` (GID {}), they are part of a previous install and are no longer required with `auto-allocate-uids = true` in nix.conf",
            self.group_name,
            self.group_id,
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "delete_users_in_group",
            group_name = self.group_name,
            group_id = self.group_id,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let mut delete_users_descriptions = Vec::new();
        for delete_user in self.delete_users.iter() {
            if let Some(val) = delete_user.describe_execute().first() {
                delete_users_descriptions.push(val.description.clone())
            }
        }

        let mut explanation = vec![
            format!("The `auto-allocate-uids` feature allows Nix to create UIDs dynamically as needed, meaning these users leftover from a previous install can be deleted"),
        ];
        explanation.append(&mut delete_users_descriptions);

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        for delete_user in self.delete_users.iter_mut() {
            delete_user.try_execute().await.map_err(Self::error)?;
        }
        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        let mut delete_users_descriptions = Vec::new();
        for delete_user in self.delete_users.iter() {
            if let Some(val) = delete_user.describe_revert().first() {
                delete_users_descriptions.push(val.description.clone())
            }
        }

        let mut explanation = vec![
            format!("The `auto-allocate-uids` feature allows Nix to create UIDs dynamically as needed, meaning these users leftover from a previous install can be deleted"),
        ];
        explanation.append(&mut delete_users_descriptions);

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        let mut errors = vec![];
        for delete_user in self.delete_users.iter_mut() {
            if let Err(err) = delete_user.try_revert().await {
                errors.push(err);
            }
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
