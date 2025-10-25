use crate::feedback::Feedback;

const PRE_PKG_SUGGEST: &str = "For a more robust Nix installation, use the Determinate package for macOS: https://dtr.mn/determinate-nix";

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
