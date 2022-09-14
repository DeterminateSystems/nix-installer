use tokio::task::JoinSet;

use crate::{settings::InstallSettings, HarmonicError};

use super::{Actionable, CreateUser, ActionReceipt, create_user::CreateUserReceipt, Revertable};


#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUsers {
    children: Vec<CreateUser>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUsersReceipt {
    children: Vec<ActionReceipt>,
}

impl CreateUsers {
    pub fn plan(nix_build_user_prefix: &String, nix_build_user_id_base: usize, daemon_user_count: usize) -> Self {
        let children = (0..daemon_user_count).map(|count| CreateUser::plan(
            format!("{nix_build_user_prefix}{count}"),
            nix_build_user_id_base + count
        )).collect();
        Self { children }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateUsers {
    fn description(&self) -> String {
        todo!()
    }

    async fn execute(self) -> Result<ActionReceipt, HarmonicError> {
        // TODO(@hoverbear): Abstract this, it will be common
        let Self { children } = self;
        let mut set = JoinSet::new();
        let mut successes = Vec::with_capacity(children.len());
        let mut errors = Vec::default();

        for child in children {
            let _abort_handle = set.spawn(async move { child.execute().await });
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
                return Err(errors.into_iter().next().unwrap())
            } else {
                return Err(HarmonicError::Multiple(errors))
            }
        }
        
        Ok(ActionReceipt::CreateUsers(CreateUsersReceipt{ children: successes }))
    }
}


#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateUsersReceipt {
    fn description(&self) -> String {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();
        
        Ok(())
    }
}
