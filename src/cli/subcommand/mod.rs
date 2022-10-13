mod plan;
use plan::Plan;
mod install;
use install::Install;
mod uninstall;
use uninstall::Uninstall;

#[derive(Debug, clap::Subcommand)]
pub(crate) enum HarmonicSubcommand {
    Plan(Plan),
    Install(Install),
    Uninstall(Uninstall),
}
