mod install;
mod plan;
mod repair;
mod self_test;
mod split_receipt;
mod uninstall;

use install::Install;
use plan::Plan;
use repair::Repair;
use self_test::SelfTest;
use split_receipt::SplitReceipt;
use uninstall::Uninstall;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub enum NixInstallerSubcommand {
    Install(Install),
    Repair(Repair),
    Uninstall(Uninstall),
    SelfTest(SelfTest),
    Plan(Plan),
    SplitReceipt(SplitReceipt),
}
