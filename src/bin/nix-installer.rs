use std::{io::IsTerminal, process::ExitCode};

use clap::Parser;
use nix_installer::cli::CommandExecute;
use nix_installer::feedback::FeedbackWorker;

#[tokio::main]
async fn main() -> eyre::Result<ExitCode> {
    color_eyre::config::HookBuilder::default()
        .issue_url(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new"))
        .add_issue_metadata("version", env!("CARGO_PKG_VERSION"))
        .add_issue_metadata("os", std::env::consts::OS)
        .add_issue_metadata("arch", std::env::consts::ARCH)
        .theme(if !std::io::stderr().is_terminal() {
            color_eyre::config::Theme::new()
        } else {
            color_eyre::config::Theme::dark()
        })
        .install()?;

    let cli = nix_installer::cli::NixInstallerCli::parse();

    let _guard = cli.instrumentation.setup()?;

    tracing::info!("nix-installer v{}", env!("CARGO_PKG_VERSION"));

    #[cfg(not(feature = "diagnostics"))]
    let (feedback, feedback_worker) = nix_installer::feedback::devnull::dev_null();

    #[cfg(feature = "diagnostics")]
    let (feedback, feedback_worker) = nix_installer::diagnostics::diagnostics(
        cli.diagnostic_attribution.clone(),
        cli.diagnostic_endpoint.clone(),
        cli.ssl_cert_file.clone(),
        cli.proxy.clone(),
    )
    .await;

    let err = cli.execute(feedback).await;

    feedback_worker.submit().await;

    err
}
