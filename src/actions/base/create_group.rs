use tokio::process::Command;

use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateGroup {
    name: String,
    gid: usize,
}

impl CreateGroup {
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

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { name, gid } = self;

        let mut command = Command::new("groupadd");

        command.args([
            "-g",
            &gid.to_string(),
            "--system",
            &name
        ]);

        let command_str = format!("{:?}", command.as_std());
        let status = command
            .status()
            .await
            .map_err(|e| HarmonicError::CommandFailedExec(command_str.clone(), e))?;
        
        match status.success() {
            true => (),
            false => return Err(HarmonicError::CommandFailedStatus(command_str)),
        }

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

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
