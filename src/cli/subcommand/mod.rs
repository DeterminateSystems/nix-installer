mod plan;
use plan::Plan;
mod install;
use install::Install;
mod restore_shell;
use restore_shell::RestoreShell;
mod uninstall;
use uninstall::Uninstall;
mod self_test;
use self_test::SelfTest;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, clap::Subcommand)]
pub enum NixInstallerSubcommand {
    Install(Install),
    RestoreShell(RestoreShell),
    Uninstall(Uninstall),
    SelfTest(SelfTest),
    Plan(Plan),
}
