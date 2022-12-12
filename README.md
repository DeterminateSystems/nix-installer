# Harmonic

> **Harmonic is pre-release and experimental.** Don't run it on machines you care about.

Harmonic is an opinionated, experimental Nix installer.

> Try it on a machine/VM you don't care about!
>
> ```bash
> curl -L https://install.determinate.systems/nix | sh -s -- install
> ```

## Status

Harmonic is **pre-release and experimental**. It is not ready for you to use! *Please* don't use it on a machine you are not planning to obliterate!

Planned support:

* [x] Multi-user x86_64 Linux with systemd init
* [x] Multi-user aarch64 Linux with systemd init
* [x] Multi-user x86_64 MacOS
    + Note: User deletion is currently unimplemented, you need to use a user with a secure token and `dscl . -delete /Users/_nixbuild*` where `*` is each user number.
* [x] Multi-user aarch64 MacOS
    + Note: User deletion is currently unimplemented, you need to use a user with a secure token and `dscl . -delete /Users/_nixbuild*` where `*` is each user number.
* [x] Valve Steam Deck
* [ ] Single-user x86_64 Linux
* [ ] Single-user aarch64 Linux
* [ ] Others...

## Installation Differences

Differing from the current official [Nix](https://github.com/NixOS/nix) installer scripts:

* Nix is installed with the `nix-command` and `flakes` features enabled in the `nix.conf`
* Harmonic stores an installation receipt (for uninstalling) at `/nix/receipt.json` as well as a copy of the install binary at `/nix/harmonic`

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

Since you'll be using Harmonic to install Nix on systems without Nix, the default build is a static binary.

Build it on a system with Nix:

```bash
nix build github:determinatesystems/harmonic
```

Then copy the `result/bin/harmonic` to the machine you wish to run it on.

## Installing

Install Nix with the default planner and options:

```bash
./harmonic install
```

> `harmonic` will elevate itself if it is not run as `root` using `sudo`. If you use `doas` or `please` you may need to elevate `harmonic` yourself.

To observe verbose logging, either use `harmonic -v`, this tool [also respects the `RUST_LOG` environment](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives). (Eg `RUST_LOG=harmonic=trace harmonic`).

Harmonic installs Nix by following a *plan* made by a *planner*. Review the available planners:

```bash
$ ./harmonic install --help
Execute an install (possibly using an existing plan)

To pass custom options, select a planner, for example `harmonic install linux-multi --help`

Usage: harmonic install [OPTIONS] [PLAN]
       harmonic install <COMMAND>

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
$ ./harmonic install linux-multi --help
A standard Linux multi-user install

Usage: harmonic install linux-multi [OPTIONS]

Options:
      --channels <channel>
          Channel(s) to add [env: HARMONIC_CHANNEL=] [default: nixpkgs=https://nixos.org/channels/nixpkgs-unstable]
      --no-confirm
          
  -v, --verbose...
          Enable debug logs, -vv for trace
      --explain
          
      --logger <LOGGER>
          Which logger to use [default: compact] [possible values: compact, full, pretty, json]
      --modify-profile
          Modify the user profile to automatically load nix [env: HARMONIC_NO_MODIFY_PROFILE=]
      --daemon-user-count <DAEMON_USER_COUNT>
          Number of build users to create [env: HARMONIC_DAEMON_USER_COUNT=] [default: 32]
      --nix-build-group-name <NIX_BUILD_GROUP_NAME>
          The Nix build group name [env: HARMONIC_NIX_BUILD_GROUP_NAME=] [default: nixbld]
      --nix-build-group-id <NIX_BUILD_GROUP_ID>
          The Nix build group GID [env: HARMONIC_NIX_BUILD_GROUP_ID=] [default: 3000]
      --nix-build-user-prefix <NIX_BUILD_USER_PREFIX>
          The Nix build user prefix (user numbers will be postfixed) [env: HARMONIC_NIX_BUILD_USER_PREFIX=] [default: nixbld]
      --nix-build-user-id-base <NIX_BUILD_USER_ID_BASE>
          The Nix build user base UID (ascending) [env: HARMONIC_NIX_BUILD_USER_ID_BASE=] [default: 3000]
      --nix-package-url <NIX_PACKAGE_URL>
          The Nix package URL [env: HARMONIC_NIX_PACKAGE_URL=] [default: https://releases.nixos.org/nix/nix-2.12.0/nix-2.12.0-x86_64-linux.tar.xz]
      --extra-conf <EXTRA_CONF>
          Extra configuration lines for `/etc/nix.conf` [env: HARMONIC_EXTRA_CONF=]
      --force
          If Harmonic should forcibly recreate files it finds existing [env: HARMONIC_FORCE=]
  -h, --help
          Print help information
```

Planners can be configured via environment variable, or by the command arguments.

```bash
$ HARMONIC_DAEMON_USER_COUNT=4 ./harmonic install linux-multi --nix-build-user-id-base 4000 --help
```

## Uninstalling

You can remove a Harmonic-installed Nix by running

```bash
/nix/harmonic uninstall
```

## As a library

> We haven't published to [crates.io](https://crates.io/) yet. We plan to for 0.0.1.

Add `harmonic` to your dependencies:

```bash
cargo add --git https://github.com/DeterminateSystems/harmonic
```

> **Building a CLI?** Check out the `cli` feature flag for `clap` integration.

Then it's possible to review the documentation:

```bash
cargo doc --open -p harmonic
```

Documentation is also available via `nix` build:

```bash
nix build github:DeterminateSystems/harmonic#harmonic.doc
firefox result-doc/harmonic/index.html
```