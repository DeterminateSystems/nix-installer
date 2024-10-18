mod install;
mod plan;
mod prepare;
mod repair;
mod self_test;
mod uninstall;

use install::Install;
use plan::Plan;
use prepare::Prepare;
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
    Prepare(Prepare),
}
