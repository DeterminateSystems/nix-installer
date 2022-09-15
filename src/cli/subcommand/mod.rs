mod plan;
use plan::Plan;
mod execute;
use execute::Execute;


#[derive(Debug, clap::Subcommand)]
pub(crate) enum HarmonicSubcommand {
    Plan(Plan),
    Execute(Execute),
}

