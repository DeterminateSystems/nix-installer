# The Determinate Nix Installer

[![Crates.io](https://img.shields.io/crates/v/nix-installer)](https://crates.io/crates/nix-installer)
[![Docs.rs](https://img.shields.io/docsrs/nix-installer)](https://docs.rs/nix-installer/latest/nix_installer/)

A fast, friendly, and reliable tool to help you use [Nix] with Flakes everywhere.


```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

The `nix-installer` has successfully completed over 2,000,000 installs in a number of environments, including [Github Actions](#as-a-github-action) and [GitLab](#on-gitlab):

| Platform                     | Multi User         | `root` only | Maturity          |
|------------------------------|:------------------:|:-----------:|:-----------------:|
| Linux (x86_64 & aarch64)     | ✓ (via [systemd])  | ✓           | Stable            |
| MacOS (x86_64 & aarch64)     | ✓                  |             | Stable (See note) |
| Valve Steam Deck (SteamOS)   | ✓                  |             | Stable            |
| WSL2 (x86_64 & aarch64)      | ✓ (via [systemd])  | ✓           | Stable            |
| Podman Linux Containers      | ✓ (via [systemd])  | ✓           | Stable            |
| Docker Containers            |                    | ✓           | Stable            |
| Linux (i686)                 | ✓ (via [systemd])  | ✓           | Unstable          |

> **Note**
> On **MacOS only**, removing users and/or groups may fail if there are no users who are logged in graphically.


## Usage

Install Nix with the default planner and options:

```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

Or, to download a platform specific Installer binary yourself:

```bash
$ curl -sL -o nix-installer https://install.determinate.systems/nix/nix-installer-x86_64-linux
$ chmod +x nix-installer
$ ./nix-installer
```

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

### Upgrading Nix

You can upgrade Nix to the current recommended version of Nix by running:

```
sudo -i nix upgrade-nix
```

Alternatively, you can [uninstall](#uninstalling) and [reinstall](#usage) with a different version of the `nix-installer`.

### Uninstalling

You can remove a `nix-installer`-installed Nix by running

```bash
/nix/nix-installer uninstall
```


### As a Github Action

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

### On GitLab

GitLab CI runners are typically Docker based and run as the `root` user. This means `systemd` is not present, so the `--init none` option needs to be passed to the Linux planner. 

On the default [GitLab.com](https://gitlab.com/) runners, `nix` can be installed and used like so:

```yaml
test:
  script:
  - curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install linux --no-confirm --init none
  - . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
  - nix run nixpkgs#hello
  - nix profile install nixpkgs#hello
  - hello
```

If you are using different runners, the above example may need to be adjusted.

### Without systemd (Linux only)

> **Warning**
> When `--init none` is used, _only_ `root` or users who can elevate to `root` privileges can run Nix:
>
> ```bash
> sudo -i nix run nixpkgs#hello
> ```

If you don't use [systemd], you can still install Nix by explicitly specifying the `linux` plan and `--init none`:

```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install linux --init none
```

### In a container

In Docker/Podman containers or WSL2 instances where an init (like `systemd`) is not present, pass `--init none`.

For containers (without an init):

> **Warning**
> When `--init none` is used, _only_ `root` or users who can elevate to `root` privileges can run Nix:
>
> ```bash
> sudo -i nix run nixpkgs#hello
> ```

```dockerfile
# Dockerfile
FROM ubuntu:latest
RUN apt update -y
RUN apt install curl -y
RUN curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install linux \
  --extra-conf "sandbox = false" \
  --init none \
  --no-confirm
ENV PATH="${PATH}:/nix/var/nix/profiles/default/bin"
RUN nix run nixpkgs#hello
```

```bash
docker build -t ubuntu-with-nix .
docker run --rm -ti ubuntu-with-nix
docker rmi ubuntu-with-nix
# or
podman build -t ubuntu-with-nix .
podman run --rm -ti ubuntu-with-nix
podman rmi ubuntu-with-nix
```

For containers with a systemd init:

```dockerfile
# Dockerfile
FROM ubuntu:latest
RUN apt update -y
RUN apt install curl systemd -y
RUN curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install linux \
  --extra-conf "sandbox = false" \
  --no-start-daemon \
  --no-confirm
ENV PATH="${PATH}:/nix/var/nix/profiles/default/bin"
RUN nix run nixpkgs#hello
CMD [ "/bin/systemd" ]
```

```bash
podman build -t ubuntu-systemd-with-nix .
IMAGE=$(podman create ubuntu-systemd-with-nix)
CONTAINER=$(podman start $IMAGE)
podman exec -ti $CONTAINER /bin/bash
podman rm -f $CONTAINER
podman rmi $IMAGE
```

On some container tools, such as `docker`, `sandbox = false` can be omitted. Omitting it will negatively impact compatibility with container tools like `podman`.

### In WSL2

We **strongly recommend** [enabling systemd](https://ubuntu.com/blog/ubuntu-wsl-enable-systemd), then installing Nix as normal:


```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

If [WSLg][wslg] is enabled, you can do things like open a Linux Firefox from Windows on Powershell:

```powershell
wsl nix run nixpkgs#firefox
```

To use some OpenGL applications, you can use [`nixGL`][nixgl] (note that some applications, such as `blender`, may not work):

```powershell
wsl nix run --impure github:guibou/nixGL nix run nixpkgs#obs-studio
```


If enabling systemd is not an option, pass `--init none` at the end of the command:

> **Warning**
> When `--init none` is used, _only_ `root` or users who can elevate to `root` privileges can run Nix:
>
> ```bash
> sudo -i nix run nixpkgs#hello
> ```


```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install linux --init none
```

### Skip confirmation

If you'd like to bypass the confirmation step, you can apply the `--no-confirm` flag:

```bash
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install --no-confirm
```

This is especially useful when using the installer in non-interactive scripts.


## Quirks

While `nix-installer` tries to provide a comprehensive and unquirky experience, there are unfortunately some issues which may require manual intervention or operator choices.

### Using MacOS after removing `nix` while `nix-darwin` was still installed, network requests fail

If `nix` was previously uninstalled without uninstalling `nix-darwin` first, users may experience errors similar to this:

```bash
$ nix shell nixpkgs#curl
error: unable to download 'https://cache.nixos.org/g8bqlgmpa4yg601w561qy2n576i6g0vh.narinfo': Problem with the SSL CA cert (path? access rights?) (77)
```

This occurs because `nix-darwin` provisions an `org.nixos.activate-system` service which remains after Nix is uninstalled.
The `org.nixos.activate-system` service in this state interacts with the newly installed Nix and changes the SSL certificates it uses to be a broken symlink.

```bash
$ ls -lah /etc/ssl/certs
total 0
drwxr-xr-x  3 root  wheel    96B Oct 17 08:26 .
drwxr-xr-x  6 root  wheel   192B Sep 16 06:28 ..
lrwxr-xr-x  1 root  wheel    41B Oct 17 08:26 ca-certificates.crt -> /etc/static/ssl/certs/ca-certificates.crt
```

The problem is compounded by the matter that the [`nix-darwin` uninstaller](https://github.com/LnL7/nix-darwin#uninstalling) will not work after uninstalling Nix, since it uses Nix and requires network connectivity.

It's possible to resolve this situation by removing the `org.nixos.activate-system` service and the `ca-certificates`:

```bash
$ sudo rm /Library/LaunchDaemons/org.nixos.activate-system.plist
$ sudo launchctl bootout system/org.nixos.activate-system
$ /nix/nix-installer uninstall
$ sudo rm /etc/ssl/certs/ca-certificates.crt
```

Then run the `nix-installer` again, and it should work.

Up-to-date versions of the `nix-installer` will refuse to uninstall until `nix-darwin` is uninstalled first, helping mitigate this problem.

## Building a binary

Since you'll be using `nix-installer` to install Nix on systems without Nix, the default build is a static binary.

Build a portable Linux binary on a system with Nix:

```bash
# to build a local copy
nix build -L ".#nix-installer-static"
# to build the remote main development branch
nix build -L "github:determinatesystems/nix-installer#nix-installer-static"
# for a specific version of the installer:
export NIX_INSTALLER_TAG="v0.6.0"
nix build -L "github:determinatesystems/nix-installer/$NIX_INSTALLER_TAG#nix-installer-static"
```

On Mac:

```bash
# to build a local copy
nix build -L ".#nix-installer"
# to build the remote main development branch
nix build -L "github:determinatesystems/nix-installer#nix-installer"
# for a specific version of the installer:
export NIX_INSTALLER_TAG="v0.6.0"
nix build -L "github:determinatesystems/nix-installer/$NIX_INSTALLER_TAG#nix-installer"
```

Then copy the `result/bin/nix-installer` to the machine you wish to run it on.

You can also add `nix-installer` to a system without Nix via `cargo`, there are no system dependencies to worry about:

```bash
# to build and run a local copy
RUSTFLAGS="--cfg tokio_unstable" cargo run -- --help
# to build the remote main development branch
RUSTFLAGS="--cfg tokio_unstable" cargo install --git https://github.com/DeterminateSystems/nix-installer
nix-installer --help
# for a specific version of the installer:
export NIX_INSTALLER_TAG="v0.6.0"
RUSTFLAGS="--cfg tokio_unstable" cargo install --git https://github.com/DeterminateSystems/nix-installer --tag $NIX_INSTALLER_TAG
nix-installer --help
```

To make this build portable, pass ` --target x86_64-unknown-linux-musl`.

> **Note**
> We currently require `--cfg tokio_unstable` as we utilize [Tokio's process groups](https://docs.rs/tokio/1.24.1/tokio/process/struct.Command.html#method.process_group), which wrap stable `std` APIs, but are unstable due to it requiring an MSRV bump.


## As a library

> **Warning**
> Use as a library is still experimental. This feature is likely to be removed in the future without an advocate. If you're using this, please let us know and we can make a path to stabilization.

Add `nix-installer` to your dependencies:

```bash
cargo add nix-installer
```

If you are **building a CLI**, check out the `cli` feature flag for `clap` integration.

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

## Accessing other versions

For users who desire version pinning, the version of `nix-installer` to use can be specified in the `curl` command:

```bash
VERSION="v0.6.0"
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix/tag/${VERSION} | sh -s -- install
```

To discover which versions are available, or download the binaries for any release, check the [Github Releases](https://github.com/DeterminateSystems/nix-installer/releases).

These releases can be downloaded and used directly:

```bash
VERSION="v0.6.0"
ARCH="aarch64-linux"
curl -sSf -L https://github.com/DeterminateSystems/nix-installer/releases/download/${VERSION}/nix-installer-${ARCH} -o nix-installer
./nix-installer install
```

Each installer version has an [associated supported nix version](src/settings.rs) -- if you pin the installer version, you'll also indirectly pin to the associated nix version.

You can also override the `nix` version via `--nix-package-url` or `NIX_INSTALLER_NIX_PACKAGE_URL=` but doing so is not recommended since we haven't tested that combination.
Here are some example `nix` package URLs including nix version, OS and architecture:

* https://releases.nixos.org/nix/nix-2.18.1/nix-2.18.1-x86_64-linux.tar.xz
* https://releases.nixos.org/nix/nix-2.18.1/nix-2.18.1-aarch64-darwin.tar.xz

## Installation Differences

Differing from the upstream [Nix](https://github.com/NixOS/nix) installer scripts:

* In `nix.conf`:
  + the `nix-command` and `flakes` features are enabled
  + `bash-prompt-prefix` is set
  + `auto-optimise-store` is set to `true` (On Linux only)
  * `extra-nix-path` is set to `nixpkgs=flake:nixpkgs`
  * `max-jobs` is set to `auto`
  * `upgrade-nix-store-path-url` is set to `https://install.determinate.systems/nix-upgrade/stable/universal`
* an installation receipt (for uninstalling) is stored at `/nix/receipt.json` as well as a copy of the install binary at `/nix/nix-installer`
* `nix-channel --update` is not run, `~/.nix-channels` is not provisioned
* `ssl-cert-file` is set in `/etc/nix/nix.conf` if the `ssl-cert-file` argument is used.

## Motivations

The existing upstream scripts do a good job, however they are difficult to maintain.

Subtle differences in the shell implementations and tool used in the scripts make it difficult to make meaningful changes to the installer.

The Determinate Nix installer has numerous advantages:

* survives macOS upgrades
* keeping an installation receipt for easy uninstallation
* offering users a chance to review an accurate, calculated install plan
* having 'planners' which can create appropriate install plans for complicated targets
* offering users with a failing install the chance to do a best-effort revert
* improving performance by maximizing parallel operations
* supporting a expanded test suite including 'curing' cases
* supporting SELinux and OSTree based distributions without asking users to make compromises
* operating as a single, static binary with external dependencies such as `openssl`, only calling existing system tools (like `useradd`) where necessary
* As a MacOS remote build target, ensures `nix` is not absent from path

It has been wonderful to collaborate with other participants in the Nix Installer Working Group and members of the broader community. The working group maintains a [foundation owned fork of the installer](https://github.com/nixos/experimental-nix-installer/).


## Diagnostics

The goal of the Determinate Nix Installer is to successfully and correctly install Nix.
The `curl | sh` pipeline and the installer collects a little bit of diagnostic information to help us make that true.

Here is a table of the [diagnostic data we collect][diagnosticdata]:

| Field                 | Use                                                                                                   |
| --------------------- | ----------------------------------------------------------------------------------------------------- |
| `version`             | The version of the Determinate Nix Installer.                                                         |
| `planner`             | The method of installing Nix (`linux`, `macos`, `steam-deck`)                                         |
| `configured_settings` | The names of planner settings which were changed from their default. Does _not_ include the values.   |
| `os_name`             | The running operating system.                                                                         |
| `os_version`          | The version of the operating system.                                                                  |
| `triple`              | The architecture/operating system/binary format of your system.                                       |
| `is_ci`               | Whether the installer is being used in CI (e.g. GitHub Actions).                                      |
| `action`              | Either `Install` or `Uninstall`.                                                                      |
| `status`              | One of `Success`, `Failure`, `Pending`, or `Cancelled`.                                               |
| `attribution`         | Optionally defined by the user, associate the diagnostics of this run to the provided value.          |
| `failure_chain`     | A high level description of what the failure was, if any. For example: `Command("diskutil")` if the command `diskutil list` failed. |

To disable diagnostic reporting, set the diagnostics URL to an empty string by passing `--diagnostic-endpoint=""` or setting `NIX_INSTALLER_DIAGNOSTIC_ENDPOINT=""`.

You can read the full privacy policy for [Determinate Systems][detsys], the creators of the Determinate Nix Installer, [here][privacy].

[detsys]: https://determinate.systems/
[diagnosticdata]: https://github.com/DeterminateSystems/nix-installer/blob/f9f927840d532b71f41670382a30cfcbea2d8a35/src/diagnostics.rs#L29-L43
[privacy]: https://determinate.systems/policies/privacy
[systemd]: https://systemd.io
[wslg]: https://github.com/microsoft/wslg
[nixgl]: https://github.com/guibou/nixGL
[Nix]: https://nixos.org
