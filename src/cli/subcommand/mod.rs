mod nixos;

#[derive(Debug, clap::Subcommand)]
pub(crate) enum Subcommand {
    #[cfg(target_os = "linux")]
    #[clap(name = "nixos")]
    NixOs(nixos::NixOsCommand),
}
