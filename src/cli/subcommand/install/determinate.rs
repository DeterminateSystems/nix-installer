use std::io::IsTerminal as _;

use owo_colors::OwoColorize as _;

use crate::cli::interaction::PromptChoice;
use crate::feedback::Feedback;
use crate::planner::BuiltinPlanner;

const PRE_PKG_SUGGEST: &str = "For a more robust Nix installation, use the Determinate package for macOS: https://dtr.mn/determinate-nix";

const INSTALL_DETERMINATE_NIX_PROMPT: &str = "\
Install Determinate Nix?

It has stable flakes, lazy trees, parallel evaluation, and more.

Selecting 'no' will install upstream Nix, which comes from NixOS.org.\
";

const DETERMINATE_MSG_EXPLAINER: &str = "\
Determinate Nix is Determinate Systems' validated and secure downstream Nix distribution for enterprises. \
It is the direct result of our work to ship meaningful user experience and reliability improvements to Nix.
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

    if !planner_settings.determinate_nix {
        // This is deliberately checking the determinate_nix option, and not `.distribution()`.
        // When we default to Determinate Nix on November 10, we'll change prefer_upstream's default to false.
        // Then, .distribution() will be Determinate Nix, but .determinate_nix will still be false.
        // That means we'll still show this warning.

        eprintln!();
        eprintln!(
            "{} The Determinate Nix Installer will stop distributing upstream Nix no sooner than {}.",
            "Important:".bold().red().italic(),
            "January 1, 2026".italic()
        );

        eprintln!("\n{}", "Timeline".bold().underline());

        eprintln!(
            "* {}: we are changing the installer to default to Determinate Nix.",
            "November 10".bold()
        );
        eprintln!(
            "  You can add the `{}` flag now to keep upstream Nix as the default.",
            "--prefer-upstream-nix".italic()
        );
        eprintln!(
            "* {}: we are removing support for installing upstream Nix.",
            "January 1".bold()
        );
        eprintln!(
            "  The `{}` flag will not have an effect any longer.",
            "--prefer-upstream-nix".italic()
        );

        eprintln!(
            "\nThe DeterminateSystems/nix-installer-action GitHub Action is also affected.\n"
        );
        eprintln!(
            "{} https://determinate.systems/blog/installer-dropping-upstream/",
            "Details:".bold().italic()
        );
    }

    if planner_settings.distribution().is_upstream()
        && std::io::stdin().is_terminal()
        && !no_confirm
    {
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
        planner_settings.distribution().is_determinate(),
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
