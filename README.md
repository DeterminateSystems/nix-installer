# Harmonic

> **Harmonic is pre-release and experimental.** Don't run it on machines you care about.

Harmonic is an opinionated, experimental Nix installer.

## Status

Harmonic is **pre-release and experimental**. It is not ready for you to use! *Please* don't use it on a machine you are not planning to obliterate!

Planned support:

* [x] Multi-user x86_64 Linux with systemd init
* [ ] Multi-user aarch64 Linux with systemd init
* [x] Multi-user x86_64 MacOS
    + Note: User deletion is currently unimplemented, you need to use a user with a secure token and `dscl . -delete /Users/_nixbuild*` where `*` is each user number.
* [x] Multi-user aarch64 MacOS
    + Note: User deletion is currently unimplemented, you need to use a user with a secure token and `dscl . -delete /Users/_nixbuild*` where `*` is each user number.
* [ ] Single-user x86_64 Linux with systemd init
* [ ] Single-user aarch64 Linux with systemd init
* [ ] Others...

## Installation Differences

Differing from the current official Nix installer scripts:

* Nix is installed with the `nix-command` and `flakes` features enabled in the `nix.conf`
* Harmonic stores an installation receipt (for uninstalling) at `/nix/receipt.json`

## Motivations

The current Nix installer scripts do an excellent job, however they are difficult to maintain. Subtle differences in the shell implementations, and certain characteristics of bash scripts make it difficult to make meaningful changes to the installer.

Our team wishes to experiment with the idea of an installer in a more structured language and see if this is a worthwhile alternative. Along the way, we are also exploring a few other ideas, such as:

* offering users a chance to review an accurate, calculated install plan
* keeping an installation receipt for uninstallation
* offering users with a failing install the chance to revert
* doing whatever tasks we can in parallel

So far, our explorations have been quite fruitful, so we wanted to share and keep exploring.

## Building

Harmonic is pre-release and we do not provide binaries at this time.

Since you'll be using Harmonic to install Nix on systems without Nix, the default build is a static binary.

Build it on a system with Nix:

```bash
nix build github:determinatesystems/harmonic
```

Then copy the `result/bin/harmonic` to the machine you wish to run it on.

## Running

Harmonic must be run as `root`, as it needs to alter the system and cannot elevate privileges without significant complexity.

Install Nix with default options:

```bash
# Linux
./harmonic install linux-multi

# Mac
./harmonic install darwin-multi
```

To observe verbose logging, either use `harmonic -v`, this tool [also respects the `RUST_LOG` environment](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives). (Eg `RUST_LOG=harmonic=trace harmonic`).

Harmonic supports many of the options the current official Nix installer scripts. Review `harmonic --help` for details.

# Uninstalling

You can remove a Harmonic-installed Nix by running

```bash
./harmonic uninstall
```

If you're on Mac and trying to run this over SSH, ensure you enable `root` by running `dsenableroot`, running the uninstall, then running `dsenableroot -d` to disable `root` again.