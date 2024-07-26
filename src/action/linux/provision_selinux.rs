use std::path::{Path, PathBuf};

use tokio::fs::{create_dir_all, remove_file};
use tokio::process::Command;
use tracing::{span, Span};

use crate::action::{ActionError, ActionErrorKind, ActionTag};
use crate::execute_command;

use crate::action::{Action, ActionDescription, StatefulAction};

const SE_LINUX_POLICY_PP_CONTENT: &[u8] = include_bytes!("selinux/nix.pp");

/**
Provision the selinux/nix.pp for SELinux compatibility
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "provision_selinux")]
pub struct ProvisionSelinux {
    policy_path: PathBuf,
}

impl ProvisionSelinux {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(policy_path: PathBuf) -> Result<StatefulAction<Self>, ActionError> {
        let this = Self { policy_path };

        // Note: `restorecon` requires us to not just skip this, even if everything is in place.

        Ok(StatefulAction::uncompleted(this))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "provision_selinux")]
impl Action for ProvisionSelinux {
    fn action_tag() -> ActionTag {
        ActionTag("provision_selinux")
    }
    fn tracing_synopsis(&self) -> String {
        "Install an SELinux Policy for Nix".to_string()
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "provision_selinux",
            policy_path = %self.policy_path.display()
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            self.tracing_synopsis(),
            vec![format!(
                "On SELinux systems (such as Fedora) a policy for Nix needs to be configured for correct operation."
            )],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        if self.policy_path.exists() {
            // Rebuild it.
            remove_existing_policy(&self.policy_path)
                .await
                .map_err(Self::error)?;
        }

        if let Some(parent) = self.policy_path.parent() {
            create_dir_all(&parent)
                .await
                .map_err(|e| ActionErrorKind::CreateDirectory(parent.into(), e))
                .map_err(Self::error)?;
        }

        tokio::fs::write(&self.policy_path, SE_LINUX_POLICY_PP_CONTENT)
            .await
            .map_err(|e| ActionErrorKind::Write(self.policy_path.clone(), e))
            .map_err(Self::error)?;

        execute_command(
            Command::new("semodule")
                .arg("--install")
                .arg(&self.policy_path),
        )
        .await
        .map_err(Self::error)?;

        execute_command(Command::new("restorecon").args(["-FR", "/nix"]))
            .await
            .map_err(Self::error)?;

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            "Remove the SELinux policy for Nix".into(),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        if self.policy_path.exists() {
            remove_existing_policy(&self.policy_path)
                .await
                .map_err(Self::error)?;
        }

        Ok(())
    }
}

async fn remove_existing_policy(policy_path: &Path) -> Result<(), ActionErrorKind> {
    execute_command(Command::new("semodule").arg("--remove").arg("nix")).await?;

    remove_file(&policy_path)
        .await
        .map_err(|e| ActionErrorKind::Remove(policy_path.into(), e))?;

    execute_command(Command::new("restorecon").args(["-FR", "/nix"])).await?;

    Ok(())
}
