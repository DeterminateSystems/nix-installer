use eyre::WrapErr;
use std::error::Error;
use std::io::IsTerminal;
use tracing_error::ErrorLayer;
use tracing_subscriber::{
    filter::Directive, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

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
    /// Which logger to use
    #[clap(long, env = "NIX_INSTALLER_LOGGER", default_value_t = Default::default(), global = true)]
    pub logger: Logger,
    /// Tracing directives
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

    pub fn setup(&self) -> eyre::Result<()> {
        let filter_layer = self.filter_layer()?;

        let registry = tracing_subscriber::registry()
            .with(filter_layer)
            .with(ErrorLayer::default());

        match self.logger {
            Logger::Compact => {
                let fmt_layer = self.fmt_layer_compact();
                registry.with(fmt_layer).try_init()?
            },
            Logger::Full => {
                let fmt_layer = self.fmt_layer_full();
                registry.with(fmt_layer).try_init()?
            },
            Logger::Pretty => {
                let fmt_layer = self.fmt_layer_pretty();
                registry.with(fmt_layer).try_init()?
            },
            Logger::Json => {
                let fmt_layer = self.fmt_layer_json();
                registry.with(fmt_layer).try_init()?
            },
        }

        Ok(())
    }

    pub fn fmt_layer_full<S>(&self) -> impl tracing_subscriber::layer::Layer<S>
    where
        S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    {
        tracing_subscriber::fmt::Layer::new()
            .with_ansi(std::io::stderr().is_terminal())
            .with_writer(std::io::stderr)
    }

    pub fn fmt_layer_pretty<S>(&self) -> impl tracing_subscriber::layer::Layer<S>
    where
        S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    {
        tracing_subscriber::fmt::Layer::new()
            .with_ansi(std::io::stderr().is_terminal())
            .with_writer(std::io::stderr)
            .pretty()
    }

    pub fn fmt_layer_json<S>(&self) -> impl tracing_subscriber::layer::Layer<S>
    where
        S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
    {
        tracing_subscriber::fmt::Layer::new()
            .with_ansi(std::io::stderr().is_terminal())
            .with_writer(std::io::stderr)
            .json()
    }

    pub fn fmt_layer_compact<S>(&self) -> impl tracing_subscriber::layer::Layer<S>
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
