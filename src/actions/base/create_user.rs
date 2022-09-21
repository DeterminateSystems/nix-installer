use tokio::process::Command;

use crate::HarmonicError;

use crate::actions::{ActionDescription, ActionReceipt, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUser {
    name: String,
    uid: usize,
    gid: usize,
}

impl CreateUser {
    pub fn plan(name: String, uid: usize, gid: usize) -> Self {
        Self { name, uid, gid }
    }
}

#[async_trait::async_trait]
impl<'a> Actionable<'a> for CreateUser {
    type Receipt = CreateUserReceipt;
    fn description(&self) -> Vec<ActionDescription> {
        let name = &self.name;
        let uid = &self.uid;
        vec![ActionDescription::new(
            format!("Create user {name} with UID {uid}"),
            vec![format!(
                "The nix daemon requires system users it can act as in order to build"
            )],
        )]
    }

    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { name, uid, gid } = self;

        let mut command = Command::new("useradd");
        command.args([
            "--home-dir",
            "/var/empty",
            "--comment",
            &format!("\"Nix build user\""),
            "--gid",
            &gid.to_string(),
            "--groups",
            &gid.to_string(),
            "--no-user-group",
            "--system",
            "--shell",
            "/sbin/nologin",
            "--uid",
            &uid.to_string(),
            "--password",
            "\"!\"",
            &name.to_string(),
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

        Ok(CreateUserReceipt { name, uid, gid })
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUserReceipt {
    name: String,
    uid: usize,
    gid: usize,
}

#[async_trait::async_trait]
impl<'a> Revertable<'a> for CreateUserReceipt {
    fn description(&self) -> Vec<ActionDescription> {
        todo!()
    }

    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
