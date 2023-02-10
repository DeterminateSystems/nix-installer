# Nix Installer

[![Crates.io](https://img.shields.io/crates/v/nix-installer)](https://crates.io/crates/nix-installer)
[![Docs.rs](https://img.shields.io/docsrs/nix-installer)](https://docs.rs/nix-installer/latest/nix_installer/)

`nix-installer` is an opinionated, **experimental** Nix installer.


```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

## Status

`nix-installer` is **pre-release and experimental**. It is not ready for high reliability use! *Please* don't use it on a business critical machine!

Current and planned support:

* [x] Multi-user Linux (aarch64 and x86_64) with systemd integration, no SELinux
* [x] Root-only Linux (aarch64 and x86_64) with no init integration, no SELinux
* [x] Multi-user MacOS (aarch64 and x86_64) with launchd integration
* [x] SteamOS on the Valve Steam Deck
* [ ] Multi-user Linux (aarch64 and x86_64) with systemd integration & SELinux
* [ ] Others...

## Installation Differences

Differing from the current official [Nix](https://github.com/NixOS/nix) installer scripts:

* Nix is installed with the `nix-command` and `flakes` features enabled in the `nix.conf`
* `nix-installer` stores an installation receipt (for uninstalling) at `/nix/receipt.json` as well as a copy of the install binary at `/nix/nix-installer`

## Motivations

The current Nix installer scripts do an excellent job, however they are difficult to maintain. Subtle differences in the shell implementations, and certain characteristics of bash scripts make it difficult to make meaningful changes to the installer.

Our team wishes to experiment with the idea of an installer in a more structured language and see if this is a worthwhile alternative. Along the way, we are also exploring a few other ideas, such as:

* offering users a chance to review an accurate, calculated install plan
* having 'planners' which can create appropriate install plans
* keeping an installation receipt for uninstallation
* offering users with a failing install the chance to do a best-effort revert
* doing whatever tasks we can in parallel

So far, our explorations have been quite fruitful, so we wanted to share and keep exploring.

## Usage

Install Nix with the default planner and options:

```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

Or, to download a platform specific Installer binary yourself:

```bash
$ curl -sL -o nix-installer https://install.determinate.systems/nix/nix-installer-x86_64-linux
$ chmod +x nix-installer
```

> `nix-installer` will elevate itself if needed using `sudo`. If you use `doas` or `please` you may need to elevate `nix-installer` yourself.

`nix-installer` installs Nix by following a *plan* made by a *planner*. Review the available planners:

```bash
$ ./nix-installer install --help
Execute an install (possibly using an existing plan)

To pass custom options, select a planner, for example `nix-installer install linux-multi --help`

Usage: nix-installer install [OPTIONS] [PLAN]
       nix-installer install <COMMAND>

Commands:
  linux
          A planner for Linux installs
  steam-deck
          A planner suitable for the Valve Steam Deck running SteamOS
  help
          Print this message or the help of the given subcommand(s)
# ...
```

Planners have their own options and defaults, sharing most of them in common:

```bash
$ ./nix-installer install linux --help
A planner for Linux installs

Usage: nix-installer install linux [OPTIONS]

Options:
# ...
      --nix-build-group-name <NIX_BUILD_GROUP_NAME>
          The Nix build group name
          
          [env: NIX_INSTALLER_NIX_BUILD_GROUP_NAME=]
          [default: nixbld]

      --nix-build-group-id <NIX_BUILD_GROUP_ID>
          The Nix build group GID
          
          [env: NIX_INSTALLER_NIX_BUILD_GROUP_ID=]
          [default: 3000]
# ...
```

Planners can be configured via environment variable or command arguments:

```bash
$ curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | NIX_BUILD_GROUP_NAME=nixbuilder sh -s -- install linux-multi --nix-build-group-id 4000
# Or...
$ NIX_BUILD_GROUP_NAME=nixbuilder ./nix-installer install linux-multi --nix-build-group-id 4000
```


## Uninstalling

You can remove a `nix-installer`-installed Nix by running

```bash
/nix/nix-installer uninstall
```


## As a Github Action

You can use the [`nix-installer-action`](https://github.com/DeterminateSystems/nix-installer-action) Github Action like so:

```yaml
on:
  pull_request:
  push:
    branches: [main]

jobs:
  lints:
    name: Build
    runs-on: ubuntu-22.04
    steps:
    - uses: actions/checkout@v3
    - name: Install Nix
      uses: DeterminateSystems/nix-installer-action@main
    - name: Run `nix build`
      run: nix build .
```

## In a container

In Docker/Podman containers or WSL instances where an init (like `systemd`) is not present, pass `--init none`.

> When `--init none` is used, only `root` or sudoers can run Nix:
>
> ```bash
> sudo -i nix run nixpkgs#hello
> ```

For Docker containers (without an init):

```dockerfile
# Dockerfile
FROM ubuntu:latest
RUN apt update -y
RUN apt install curl -y
COPY nix-installer /nix-installer
RUN /nix-installer install linux --init none --no-confirm
ENV PATH="${PATH}:/nix/var/nix/profiles/default/bin"
RUN nix run nixpkgs#hello
```

Podman containers require `sandbox = false` in your `Nix.conf`.

For podman containers without an init:

```dockerfile
# Dockerfile
FROM ubuntu:latest
RUN apt update -y
RUN apt install curl -y
COPY nix-installer /nix-installer
RUN /nix-installer install linux --extra-conf "sandbox = false" --init none --no-confirm
ENV PATH="${PATH}:/nix/var/nix/profiles/default/bin"
RUN nix run nixpkgs#hello
```

For Podman containers with an init:

```dockerfile
# Dockerfile
FROM ubuntu:latest
RUN apt update -y
RUN apt install curl systemd -y
COPY nix-installer /nix-installer
RUN /nix-installer install linux --extra-conf "sandbox = false" --no-start-daemon --no-confirm
ENV PATH="${PATH}:/nix/var/nix/profiles/default/bin"
RUN nix run nixpkgs#hello
CMD [ "/usr/sbin/init" ]
```

## In WSL

If [systemd is enabled](https://ubuntu.com/blog/ubuntu-wsl-enable-systemd) it's possible to install Nix as normal using the command at the top of this document:

```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

If systemd is not enabled, pass `--init none` at the end of the command:

> When `--init none` is used, only `root` or sudoers can run Nix:
>
> ```bash
> sudo -i nix run nixpkgs#hello
> ```


```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install linux --init none
```

## Building a binary

Since you'll be using `nix-installer` to install Nix on systems without Nix, the default build is a static binary.

Build a portable Linux binary on a system with Nix:

```bash
nix build -L github:determinatesystems/nix-installer#nix-installer-static
```

On Mac:

```bash
nix build -L github:determinatesystems/nix-installer#nix-installer
```

Then copy the `result/bin/nix-installer` to the machine you wish to run it on.

You can also add `nix-installer` to a system without Nix via `cargo`:

```bash
RUSTFLAGS="--cfg tokio_unstable" cargo install nix-installer
nix-installer --help
```

To make this build portable, pass ` --target x86_64-unknown-linux-musl`.

> We currently require `--cfg tokio_unstable` as we utilize [Tokio's process groups](https://docs.rs/tokio/1.24.1/tokio/process/struct.Command.html#method.process_group), which wrap stable `std` APIs, but are unstable due to it requiring an MSRV bump.


## As a library

Add `nix-installer` to your dependencies:

```bash
cargo add nix-installer
```

> **Building a CLI?** Check out the `cli` feature flag for `clap` integration.

You'll also need to edit your `.cargo/config.toml` to use `tokio_unstable` as we utilize [Tokio's process groups](https://docs.rs/tokio/1.24.1/tokio/process/struct.Command.html#method.process_group), which wrap stable `std` APIs, but are unstable due to it requiring an MSRV bump:

```toml
# .cargo/config.toml
[build]
rustflags=["--cfg", "tokio_unstable"]
```

Then it's possible to review the [documentation](https://docs.rs/nix-installer/latest/nix_installer/):

```bash
cargo doc --open -p nix-installer
```

Documentation is also available via `nix` build:

```bash
nix build github:DeterminateSystems/nix-installer#nix-installer.doc
firefox result-doc/nix-installer/index.html
```


