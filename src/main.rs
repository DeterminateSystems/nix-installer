use std::process::ExitCode;

use clap::Parser;
use harmonic::cli::CommandExecute;

#[tokio::main]
async fn main() -> color_eyre::Result<ExitCode> {
    color_eyre::config::HookBuilder::default()
        .theme(if !atty::is(atty::Stream::Stderr) {
            color_eyre::config::Theme::new()
        } else {
            color_eyre::config::Theme::dark()
        })
        .install()?;

    let cli = harmonic::cli::HarmonicCli::parse();

    cli.instrumentation.setup()?;

    cli.execute().await
}
