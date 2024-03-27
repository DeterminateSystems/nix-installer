use tracing::{span, Span};

use crate::action::{Action, ActionDescription, ActionError, ActionTag, StatefulAction};

/// TODO
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ApplyNixDarwin {
    nix_darwin_flake_ref: String,
}

impl ApplyNixDarwin {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn plan(nix_darwin_flake_ref: String) -> Result<StatefulAction<Self>, ActionError> {
        Ok(StatefulAction::completed(Self {
            nix_darwin_flake_ref,
        }))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "apply_nix_darwin")]
impl Action for ApplyNixDarwin {
    fn action_tag() -> ActionTag {
        ActionTag("apply_nix_darwin")
    }

    fn tracing_synopsis(&self) -> String {
        format!(
            "Apply the nix-darwin configuration from the flake ref {}`",
            self.nix_darwin_flake_ref,
        )
    }

    fn tracing_span(&self) -> Span {
        span!(
            tracing::Level::DEBUG,
            "apply_nix_darwin",
            nix_darwin_flake_ref = self.nix_darwin_flake_ref,
        )
    }

    fn execute_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(self.tracing_synopsis(), vec![])]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn execute(&mut self) -> Result<(), ActionError> {
        Ok(())
    }

    fn revert_description(&self) -> Vec<ActionDescription> {
        vec![ActionDescription::new(
            format!(
                "Revert nix-darwin apply for the flake ref {}",
                self.nix_darwin_flake_ref,
            ),
            vec![],
        )]
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn revert(&mut self) -> Result<(), ActionError> {
        Ok(())
    }
}
