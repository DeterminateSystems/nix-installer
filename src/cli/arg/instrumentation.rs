use atty::Stream;
use eyre::WrapErr;
use std::error::Error;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use valuable::Valuable;

#[derive(clap::Args, Debug, Valuable)]
pub(crate) struct Instrumentation {
    /// Enable debug logs, -vv for trace
    #[clap(
        short = 'v',
        long,
        parse(from_occurrences),
        global = true,
        group = "verbosity"
    )]
    pub(crate) verbose: usize,
}

impl<'a> Instrumentation {
    pub(crate) fn log_level(&self) -> String {
        match self.verbose {
            0 => "info",
            1 => "debug",
            _ => "trace",
        }
        .to_string()
    }

    pub(crate) fn setup<'b: 'a>(&'b self) -> eyre::Result<()> {
        let fmt_layer = self.fmt_layer();
        let filter_layer = self.filter_layer()?;

        tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt_layer)
            .with(ErrorLayer::default())
            .try_init()?;

        Ok(())
    }

    pub(crate) fn fmt_layer<S>(&self) -> impl tracing_subscriber::layer::Layer<S>
    where
        S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    {
        tracing_subscriber::fmt::Layer::new()
            .with_ansi(atty::is(Stream::Stderr))
            .with_writer(std::io::stderr)
            .pretty()
    }

    pub(crate) fn filter_layer(&self) -> eyre::Result<EnvFilter> {
        let filter_layer = match EnvFilter::try_from_default_env() {
            Ok(layer) => layer,
            Err(e) => {
                // Catch a parse error and report it, ignore a missing env.
                if let Some(source) = e.source() {
                    match source.downcast_ref::<std::env::VarError>() {
                        Some(std::env::VarError::NotPresent) => (),
                        _ => return Err(e).wrap_err_with(|| "parsing RUST_LOG directives"),
                    }
                }
                EnvFilter::try_new(&format!("{}={}", env!("CARGO_PKG_NAME"), self.log_level()))?
            }
        };

        Ok(filter_layer)
    }
}
