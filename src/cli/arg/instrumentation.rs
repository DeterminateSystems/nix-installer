use std::error::Error;
use std::io::IsTerminal;

use eyre::WrapErr;
use tracing_error::ErrorLayer;
use tracing_subscriber::filter::Directive;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer as _};

#[derive(Clone, Default, Debug, clap::ValueEnum)]
pub enum Logger {
    #[default]
    Compact,
    Full,
    Pretty,
    Json,
}

impl std::fmt::Display for Logger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let logger = match self {
            Logger::Compact => "compact",
            Logger::Full => "full",
            Logger::Pretty => "pretty",
            Logger::Json => "json",
        };
        write!(f, "{}", logger)
    }
}

#[derive(clap::Args, Debug, Default)]
pub struct Instrumentation {
    /// Enable debug logs, -vv for trace
    #[clap(short = 'v', env = "NIX_INSTALLER_VERBOSITY", long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
    /// Which logger to use (options are `compact`, `full`, `pretty`, and `json`)
    #[clap(long, env = "NIX_INSTALLER_LOGGER", default_value_t = Default::default(), global = true)]
    pub logger: Logger,
    /// Tracing directives delimited by comma
    ///
    /// See https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives
    #[clap(long = "log-directive", global = true, env = "NIX_INSTALLER_LOG_DIRECTIVES", value_delimiter = ',', num_args = 0..)]
    pub log_directives: Vec<Directive>,
}

impl Instrumentation {
    pub fn log_level(&self) -> String {
        match self.verbose {
            0 => "info",
            1 => "debug",
            _ => "trace",
        }
        .to_string()
    }

    pub fn setup(&self) -> eyre::Result<tracing_appender::non_blocking::WorkerGuard> {
        let log_path = format!("/tmp/nix-installer.log");
        let trace_log_file = std::fs::File::create(&log_path)?;
        let (nonblocking, guard) = tracing_appender::non_blocking(trace_log_file);
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(nonblocking)
            .with_filter(EnvFilter::new(format!(
                "{}=trace",
                env!("CARGO_PKG_NAME").replace('-', "_"),
            )));

        let registry = tracing_subscriber::registry()
            .with(ErrorLayer::default())
            .with(file_layer);

        let filter_layer = self.filter_layer()?;

        match self.logger {
            Logger::Compact => {
                let fmt_layer = self.fmt_layer_compact(filter_layer);
                registry.with(fmt_layer).try_init()?;
            },
            Logger::Full => {
                let fmt_layer = self.fmt_layer_full(filter_layer);
                registry.with(fmt_layer).try_init()?;
            },
            Logger::Pretty => {
                let fmt_layer = self.fmt_layer_pretty(filter_layer);
                registry.with(fmt_layer).try_init()?;
            },
            Logger::Json => {
                let fmt_layer = self.fmt_layer_json(filter_layer);
                registry.with(fmt_layer).try_init()?;
            },
        }

        Ok(guard)
    }

    pub fn fmt_layer_full<S>(&self, filter: EnvFilter) -> impl tracing_subscriber::layer::Layer<S>
    where
        S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    {
        tracing_subscriber::fmt::Layer::new()
            .with_ansi(std::io::stderr().is_terminal())
            .with_writer(std::io::stderr)
            .with_filter(filter)
    }

    pub fn fmt_layer_pretty<S>(&self, filter: EnvFilter) -> impl tracing_subscriber::layer::Layer<S>
    where
        S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    {
        tracing_subscriber::fmt::Layer::new()
            .with_ansi(std::io::stderr().is_terminal())
            .with_writer(std::io::stderr)
            .pretty()
            .with_filter(filter)
    }

    pub fn fmt_layer_json<S>(&self, filter: EnvFilter) -> impl tracing_subscriber::layer::Layer<S>
    where
        S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    {
        tracing_subscriber::fmt::Layer::new()
            .with_ansi(std::io::stderr().is_terminal())
            .with_writer(std::io::stderr)
            .json()
            .with_filter(filter)
    }

    pub fn fmt_layer_compact<S>(
        &self,
        filter: EnvFilter,
    ) -> impl tracing_subscriber::layer::Layer<S>
    where
        S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    {
        tracing_subscriber::fmt::Layer::new()
            .with_ansi(std::io::stderr().is_terminal())
            .with_writer(std::io::stderr)
            .compact()
            .without_time()
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .with_file(false)
            .with_line_number(false)
            .with_filter(filter)
    }

    pub fn filter_layer(&self) -> eyre::Result<EnvFilter> {
        let mut filter_layer = match EnvFilter::try_from_default_env() {
            Ok(layer) => layer,
            Err(e) => {
                // Catch a parse error and report it, ignore a missing env.
                if let Some(source) = e.source() {
                    match source.downcast_ref::<std::env::VarError>() {
                        Some(std::env::VarError::NotPresent) => (),
                        _ => return Err(e).wrap_err_with(|| "parsing RUST_LOG directives"),
                    }
                }
                EnvFilter::try_new(format!(
                    "{}={}",
                    env!("CARGO_PKG_NAME").replace('-', "_"),
                    self.log_level()
                ))?
            },
        };

        for directive in &self.log_directives {
            let directive_clone = directive.clone();
            filter_layer = filter_layer.add_directive(directive_clone);
        }

        Ok(filter_layer)
    }
}
