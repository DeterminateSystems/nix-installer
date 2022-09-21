use tokio::process::Command;

use crate::{HarmonicError, execute_command};

use crate::actions::{ActionDescription, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateGroup {
    name: String,
    gid: usize,
}

impl CreateGroup {
    #[tracing::instrument(skip_all)]
    pub fn plan(name: String, gid: usize) -> Self {
        Self { name, gid }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateGroup {
    type Receipt = CreateGroupReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let Self { name, gid } = &self;
        vec![ActionDescription::new(
            format!("Create group {name} with GID {gid}"),
            vec![format!(
                "The nix daemon requires a system user group its system users can be part of"
            )],
        )]
    }

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { name, gid } = self;

        execute_command(
            Command::new("groupadd").args(["-g", &gid.to_string(), "--system", &name]),
            false,
        ).await?;

        Ok(CreateGroupReceipt { name, gid })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateGroupReceipt {
    name: String,
    gid: usize,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateGroupReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
