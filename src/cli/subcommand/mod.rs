mod plan;
use plan::Plan;
mod install;
use install::Execute;
mod uninstall;
use uninstall::Uninstall;

#[derive(Debug, clap::Subcommand)]
pub(crate) enum HarmonicSubcommand {
    Plan(Plan),
    Execute(Execute),
    Uninstall(Uninstall),
}
