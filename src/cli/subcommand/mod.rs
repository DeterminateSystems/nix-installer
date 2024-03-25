mod plan;
use plan::Plan;
mod install;
use install::Install;
mod repair;
use repair::Repair;
mod uninstall;
use uninstall::Uninstall;
mod self_test;
use self_test::SelfTest;
mod export;
pub use export::Export;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub enum NixInstallerSubcommand {
    Install(Install),
    Repair(Repair),
    Uninstall(Uninstall),
    SelfTest(SelfTest),
    Plan(Plan),
    Export(Export),
}
