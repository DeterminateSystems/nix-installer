use std::io::IsTerminal as _;

use owo_colors::OwoColorize as _;

use crate::cli::interaction::PromptChoice;
use crate::feedback::Feedback;
use crate::planner::BuiltinPlanner;

const PRE_PKG_SUGGEST: &str = "For a more robust Nix installation, use the Determinate package for macOS: https://dtr.mn/determinate-nix";

const INSTALL_DETERMINATE_NIX_PROMPT: &str = "\
Install Determinate Nix?\
\
Selecting 'no' will install Nix from NixOS.org, without automated garbage collection and enterprise certificate support.\
";

const DETERMINATE_MSG_EXPLAINER: &str = "\
Determinate Nix is Determinate Systems' validated and secure downstream Nix distribution for enterprises. \
It comes bundled with Determinate Nixd, a helpful daemon that automates some otherwise-unpleasant aspects of using Nix, such as garbage collection, and enables you to easily authenticate with FlakeHub.

For more details: https://dtr.mn/determinate-nix\
";

pub(crate) async fn inform_macos_about_pkg<T: Feedback>(feedback: &T) {
    if matches!(
        target_lexicon::OperatingSystem::host(),
        target_lexicon::OperatingSystem::MacOSX { .. } | target_lexicon::OperatingSystem::Darwin
    ) {
        let msg = feedback
            .get_feature_ptr_payload::<String>("dni-det-msg-start-pkg-ptr")
            .await
            .unwrap_or(PRE_PKG_SUGGEST.into());
        tracing::info!("{}", msg.trim());
    }
}

pub(crate) async fn prompt_for_determinate<T: Feedback>(
    feedback: &mut T,
    planner: &mut BuiltinPlanner,
    no_confirm: bool,
) -> eyre::Result<Option<String>> {
    let planner_settings = planner.common_settings_mut();

    if !planner_settings.determinate_nix && std::io::stdin().is_terminal() && !no_confirm {
        let base_prompt = feedback
            .get_feature_ptr_payload::<String>("dni-det-msg-interactive-prompt-ptr")
            .await
            .unwrap_or(INSTALL_DETERMINATE_NIX_PROMPT.into());

        let mut explanation: Option<String> = None;

        loop {
            let prompt = if let Some(ref explanation) = explanation {
                &format!("\n{}\n{}", base_prompt.trim().green(), explanation.trim())
            } else {
                &format!("\n{}", base_prompt.trim().green())
            };

            let response = crate::cli::interaction::prompt(
                prompt.to_string(),
                PromptChoice::Yes,
                explanation.is_some(),
            )
            .await?;

            match response {
                PromptChoice::Explain => {
                    explanation = Some(
                        feedback
                            .get_feature_ptr_payload::<String>(
                                "dni-det-msg-interactive-explanation-ptr",
                            )
                            .await
                            .unwrap_or(DETERMINATE_MSG_EXPLAINER.into()),
                    );
                },
                PromptChoice::Yes => {
                    planner_settings.determinate_nix = true;
                    break;
                },
                PromptChoice::No => {
                    break;
                },
            }
        }
    }

    let post_install_message_feature = match (
        planner_settings.determinate_nix,
        std::io::stdin().is_terminal() && !no_confirm,
    ) {
        (true, true) => Some("dni-post-det-int-ptr"),
        (true, false) => None,
        (false, true) => Some("dni-post-ups-int-ptr"),
        (false, false) => Some("dni-post-ups-scr-ptr"),
    };

    let msg = if let Some(feat) = post_install_message_feature {
        feedback.get_feature_ptr_payload::<String>(feat).await
    } else {
        None
    };

    Ok(msg)
}
