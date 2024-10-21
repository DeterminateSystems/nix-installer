mod install;
mod make_determinate;
mod plan;
mod repair;
mod self_test;
mod uninstall;

use install::Install;
use make_determinate::MakeDeterminate;
use plan::Plan;
use repair::Repair;
use self_test::SelfTest;
use uninstall::Uninstall;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub enum NixInstallerSubcommand {
    Install(Install),
    Repair(Repair),
    Uninstall(Uninstall),
    SelfTest(SelfTest),
    Plan(Plan),
    MakeDeterminate(MakeDeterminate),
}
