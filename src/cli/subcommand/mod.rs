mod plan;
use plan::Plan;
mod install;
use install::Install;
mod uninstall;
use uninstall::Uninstall;
mod self_test;
use self_test::SelfTest;

#[derive(Debug, clap::Subcommand)]
pub enum NixInstallerSubcommand {
    Plan(Plan),
    Install(Install),
    Uninstall(Uninstall),
    SelfTest(SelfTest),
}
