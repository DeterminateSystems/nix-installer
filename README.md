# Nix Installer

[![Crates.io](https://img.shields.io/crates/v/nix-installer)](https://crates.io/crates/nix-installer)
[![Docs.rs](https://img.shields.io/docsrs/nix-installer)](https://docs.rs/nix-installer/latest/nix_installer/)

`nix-installer` is an opinionated, **experimental** Nix installer.

> Try it on a machine/VM you don't care about!
>
> ```bash
> curl -L https://install.determinate.systems/nix | sh -s -- install
> ```

## Status

`nix-installer` is **pre-release and experimental**. It is not ready for high reliability use! *Please* don't use it on a business critical machine!

Planned support:

* [x] Multi-user x86_64 Linux with systemd init, no SELinux
* [x] Multi-user aarch64 Linux with systemd init, no SELinux
* [x] Multi-user x86_64 MacOS
    + Note: User deletion is currently unimplemented, you need to use a user with a secure token and `dscl . -delete /Users/_nixbuild*` where `*` is each user number.
* [x] Multi-user aarch64 MacOS
    + Note: User deletion is currently unimplemented, you need to use a user with a secure token and `dscl . -delete /Users/_nixbuild*` where `*` is each user number.
* [x] Valve Steam Deck
* [ ] Multi-user x86_64 Linux with systemd init, with SELinux
* [ ] Multi-user aarch64 Linux with systemd init, with SELinux
* [ ] Single-user x86_64 Linux
* [ ] Single-user aarch64 Linux
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

## Building

Since you'll be using `nix-installer` to install Nix on systems without Nix, the default build is a static binary.

Build it on a system with Nix:

```bash
nix build github:determinatesystems/nix-installer
```

Then copy the `result/bin/nix-installer` to the machine you wish to run it on.

If you don't have Nix, yet still want to contribute, you can also run `cargo build` like a normal Rust crate.

## Installing

Install Nix with the default planner and options:

```bash
./nix-installer install
```

> `nix-installer` will elevate itself if it is not run as `root` using `sudo`. If you use `doas` or `please` you may need to elevate `nix-installer` yourself.

To observe verbose logging, either use `nix-installer -v`, this tool [also respects the `RUST_LOG` environment](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives). (Eg `RUST_LOG=nix_installer=trace nix-installer`).

`nix-installer` installs Nix by following a *plan* made by a *planner*. Review the available planners:

```bash
$ ./nix-installer install --help
Execute an install (possibly using an existing plan)

To pass custom options, select a planner, for example `nix-installer install linux-multi --help`

Usage: nix-installer install [OPTIONS] [PLAN]
       nix-installer install <COMMAND>

Commands:
  linux-multi
          A standard Linux multi-user install
  darwin-multi
          A standard MacOS (Darwin) multi-user install
  steam-deck
          A specialized install suitable for the Valve Steam Deck console
  help
          Print this message or the help of the given subcommand(s)
# ...
```

Planners have their own options and defaults, sharing most of them in common:

```bash
$ ./nix-installer install linux-multi --help
A standard Linux multi-user install

Usage: nix-installer install linux-multi [OPTIONS]

Options:
      --channel [<CHANNELS>...]
          Channel(s) to add, for no default channel, pass `--channel`
          
          [env: NIX_INSTALLER_CHANNELS=]
          [default: nixpkgs=https://nixos.org/channels/nixpkgs-unstable]

      --no-confirm
          [env: NIX_INSTALLER_NO_CONFIRM=]

  -v, --verbose...
          Enable debug logs, -vv for trace
          
          [env: NIX_INSTALLER_VERBOSITY=]

      --logger <LOGGER>
          Which logger to use
          
          [env: NIX_INSTALLER_LOGGER=]
          [default: compact]
          [possible values: compact, full, pretty, json]

      --modify-profile
          Modify the user profile to automatically load nix
          
          [env: NIX_INSTALLER_NO_MODIFY_PROFILE=]

      --log-directive [<LOG_DIRECTIVES>...]
          Tracing directives
          
          See https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives
          
          [env: NIX_INSTALLER_LOG_DIRECTIVES=]

      --nix-build-user-count <NIX_BUILD_USER_COUNT>
          Number of build users to create
          
          [env: NIX_INSTALLER_NIX_BUILD_USER_COUNT=]
          [default: 32]

      --nix-build-group-name <NIX_BUILD_GROUP_NAME>
          The Nix build group name
          
          [env: NIX_INSTALLER_NIX_BUILD_GROUP_NAME=]
          [default: nixbld]

      --nix-build-group-id <NIX_BUILD_GROUP_ID>
          The Nix build group GID
          
          [env: NIX_INSTALLER_NIX_BUILD_GROUP_ID=]
          [default: 3000]

      --nix-build-user-prefix <NIX_BUILD_USER_PREFIX>
          The Nix build user prefix (user numbers will be postfixed)
          
          [env: NIX_INSTALLER_NIX_BUILD_USER_PREFIX=]
          [default: nixbld]

      --nix-build-user-id-base <NIX_BUILD_USER_ID_BASE>
          The Nix build user base UID (ascending)
          
          [env: NIX_INSTALLER_NIX_BUILD_USER_ID_BASE=]
          [default: 3000]

      --nix-package-url <NIX_PACKAGE_URL>
          The Nix package URL
          
          [env: NIX_INSTALLER_NIX_PACKAGE_URL=]
          [default: https://releases.nixos.org/nix/nix-2.12.0/nix-2.12.0-x86_64-linux.tar.xz]

      --extra-conf [<EXTRA_CONF>...]
          Extra configuration lines for `/etc/nix.conf`
          
          [env: NIX_INSTALLER_EXTRA_CONF=]

      --force
          If `nix-installer` should forcibly recreate files it finds existing
          
          [env: NIX_INSTALLER_FORCE=]

      --explain
          [env: NIX_INSTALLER_EXPLAIN=]

  -h, --help
          Print help information (use `-h` for a summary)
```

Planners can be configured via environment variable, or by the command arguments.

```bash
$ NIX_INSTALLER_DAEMON_USER_COUNT=4 ./nix-installer install linux-multi --nix-build-user-id-base 4000 --help
```

## Uninstalling

You can remove a `nix-installer`-installed Nix by running

```bash
/nix/nix-installer uninstall
```

## As a library

Add `nix-installer` to your dependencies:

```bash
cargo add nix-installer
```

> **Building a CLI?** Check out the `cli` feature flag for `clap` integration.

Then it's possible to review the [documentation](https://docs.rs/nix-installer/latest/nix_installer/):

```bash
cargo doc --open -p nix-installer
```

Documentation is also available via `nix` build:

```bash
nix build github:DeterminateSystems/nix-installer#nix-installer.doc
firefox result-doc/nix-installer/index.html
```

## As a Github Action

You can use `nix-installer` as a Github action like so:

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
      uses: DeterminateSystems/nix-installer@main
      with:
        # Allow the installed Nix to make authenticated Github requests.
        # If you skip this, you will likely get rate limited.
        github-token: ${{ secrets.GITHUB_TOKEN }}
    - name: Run `nix build`
      run: nix build .
```
