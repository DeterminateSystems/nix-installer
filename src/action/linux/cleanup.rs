use std::path::Path;

use tracing::{span, Span};

use crate::action::base::setup_default_profile::DEFAULT_PROFILE_PATH;
use crate::action::base::RemoveDirectory;
use crate::action::{Action, ActionDescription, ActionErrorKind};
use crate::action::{ActionError, StatefulAction};
use crate::cli::ORIG_HOME_ENV;

/** Cleanup after a successful installation. Does nothing on revert.
*/
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "action_name", rename = "cleanup")]
pub struct Cleanup {
    remove_scratch_dir: StatefulAction<RemoveDirectory>,
    original_home_dir: Option<String>,
}

impl Cleanup {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan() -> Result<StatefulAction<Self>, ActionError> {
        let remove_scratch_dir = RemoveDirectory::plan(crate::settings::SCRATCH_DIR)
            .await
            .map_err(Self::error)?;

        let original_home_dir = std::env::var(ORIG_HOME_ENV).ok();

        Ok(StatefulAction::uncompleted(Self {
            remove_scratch_dir,
            original_home_dir,
        }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "cleanup")]
impl Action for Cleanup {
    fn action_tag() -> crate::action::ActionTag {
        crate::action::ActionTag("cleanup")
    }
    fn tracing_synopsis(&self) -> String {
        String::from("Cleanup")
    }

    fn tracing_span(&self) -> Span {
        span!(tracing::Level::DEBUG, "cleanup",)
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        let mut explanation = vec![self.remove_scratch_dir.tracing_synopsis()];

        if let Some(dir) = &self.original_home_dir {
            explanation.push(format!(
                "Remove conflicting packages from the single-user profile at `{dir}/.nix-profile`"
            ));
        }

        vec![ActionDescription::new(self.tracing_synopsis(), explanation)]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        self.remove_scratch_dir.try_execute().await?;

        if let Some(dir) = &self.original_home_dir {
            let home = Path::new(dir);
            let single_user_profile = home.join(".nix-profile");

            // NOTE(cole-h): This does expose us to TOCTOU issues, but Nix will create the profile
            // if it didn't already exist, and we don't really want to do that. So, just don't try
            // to remove Nix from the old, single-user profile if `$HOME/.nix-profile` doesn't
            // exist.
            if let Some(single_user_profile) = single_user_profile
                .exists()
                .then(|| single_user_profile.read_link().ok())
                .flatten()
            {
                let default_profile = Path::new(DEFAULT_PROFILE_PATH);

                crate::profile::nixenv::NixEnv {
                    nix_store_path: default_profile,
                    profile: &single_user_profile,
                    pkgs: &[default_profile],
                }
                .remove_conflicts(crate::profile::WriteToDefaultProfile::Specific)
                .await
                .map_err(|e| ActionErrorKind::Custom(Box::new(e)))
                .map_err(Self::error)?;
            }
        }

        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        Ok(())
    }
}
