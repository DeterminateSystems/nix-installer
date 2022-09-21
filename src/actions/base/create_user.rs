use tokio::process::Command;

use crate::{HarmonicError, execute_command};

use crate::actions::{ActionDescription, Actionable, Revertable};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct CreateUser {
    name: String,
    uid: usize,
    gid: usize,
}

impl CreateUser {
    #[tracing::instrument(skip_all)]
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

    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<Self::Receipt, HarmonicError> {
        let Self { name, uid, gid } = self;

        execute_command(Command::new("useradd").args([
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
        ]), false).await?;

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

    #[tracing::instrument(skip_all)]
    async fn revert(self) -> Result<(), HarmonicError> {
        todo!();

        Ok(())
    }
}
