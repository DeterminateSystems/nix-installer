## Building a binary

Since you'll be using the installer to install Nix on systems without Nix, the default build is a static binary.
This guide shows you how to build the installer on [Linux](#on-linux) and [macOS](#on-macos).

## On Linux

To build a portable Linux binary on a system with Nix:

```shell
# to build a local copy
nix build -L ".#nix-installer-static"
# to build the remote main development branch
nix build -L "github:determinatesystems/nix-installer#nix-installer-static"
# for a specific version of the installer:
export NIX_INSTALLER_TAG="v3.11.2"
nix build -L "github:determinatesystems/nix-installer/$NIX_INSTALLER_TAG#nix-installer-static"
```

## On macOS

```shell
# to build a local copy
nix build -L ".#nix-installer"
# to build the remote main development branch
nix build -L "github:determinatesystems/nix-installer#nix-installer"
# for a specific version of the installer:
export NIX_INSTALLER_TAG="v3.11.2"
nix build -L "github:determinatesystems/nix-installer/$NIX_INSTALLER_TAG#nix-installer"
```

## Copying the executable

Once Nix has built the executable for the desired system, you can copy `result/bin/nix-installer` to the machine you wish to run it on (in Nix, `result` is a symlink to a directory in the Nix store).
You can also add the installer to a system without Nix using [cargo], as there are no system dependencies to worry about:

```shell
# to build and run a local copy
RUSTFLAGS="--cfg tokio_unstable" cargo run -- --help
# to build the remote main development branch
RUSTFLAGS="--cfg tokio_unstable" cargo install --git https://github.com/DeterminateSystems/nix-installer
nix-installer --help
# for a specific version of the installer:
export NIX_INSTALLER_TAG="v3.11.2"
RUSTFLAGS="--cfg tokio_unstable" cargo install --git https://github.com/DeterminateSystems/nix-installer --tag $NIX_INSTALLER_TAG
nix-installer --help
```

To make this build portable, pass the `--target x86_64-unknown-linux-musl` option.

> [!NOTE]
> We currently require `--cfg tokio_unstable` as we utilize [Tokio's process groups](https://docs.rs/tokio/1.24.1/tokio/process/struct.Command.html#method.process_group), which wrap stable `std` APIs, but are unstable due to it requiring an MSRV bump.

[cargo]: https://doc.rust-lang.org/cargo
