# Determinate Nix Installer

[![Crates.io](https://img.shields.io/crates/v/nix-installer)](https://crates.io/crates/nix-installer)
[![Docs.rs](https://img.shields.io/docsrs/nix-installer)](https://docs.rs/nix-installer/latest/nix_installer)

**Determinate Nix Installer** is a fast, friendly, and reliable way to install and manage [Nix] everywhere, including macOS, Linux, Windows Subsystem for Linux (WSL), SELinux, the Valve Steam Deck, and more.
It offers support for seamlessly [uninstalling Nix](#uninstalling)

This one-liner is the quickest way to get started on any supported system:

```shell
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

Determinate Nix Installer has successfully completed over 2,000,000 installs in a number of environments, including [Github Actions](#as-a-github-action) and [GitLab](#on-gitlab):

| Platform                                                             |    Multi user?    | `root` only |     Maturity      |
| -------------------------------------------------------------------- | :---------------: | :---------: | :---------------: |
| Linux (`x86_64` and `aarch64`)                                       | ✓ (via [systemd]) |      ✓      |      Stable       |
| MacOS (`x86_64` and `aarch64`)                                       |         ✓         |             | Stable (See note) |
| [Valve Steam Deck][steam-deck] (SteamOS)                             |         ✓         |             |      Stable       |
| [Windows Subsystem for Linux][wsl] 2 (WSL2) (`x86_64` and `aarch64`) | ✓ (via [systemd]) |      ✓      |      Stable       |
| [Podman] Linux Containers                                            | ✓ (via [systemd]) |      ✓      |      Stable       |
| [Docker] Containers                                                  |                   |      ✓      |      Stable       |

## Install Nix

You can install Nix with the default [planner](#planners) and options by running this script:

```shell
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

To download a platform-specific installer binary yourself:

```shell
curl -sL -o nix-installer https://install.determinate.systems/nix/nix-installer-x86_64-linux
chmod +x nix-installer
./nix-installer
```

This would install Nix on an `x86_64-linux` system but you can replace that with the system of your choice.

### Planners

Determinate Nix Installer installs Nix by following a _plan_ made by a _planner_.
To review the available planners:

```shell
/nix/nix-installer install --help
```

Planners have their own options and defaults, sharing most of them in common.
To see the options for Linux, for example:

```shell
/nix/nix-installer install linux --help
```

You can configure planners using environment variables or command arguments:

```shell
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | NIX_BUILD_GROUP_NAME=nixbuilder sh -s -- install --nix-build-group-id 4000

# Or...

NIX_BUILD_GROUP_NAME=nixbuilder ./nix-installer install --nix-build-group-id 4000
```

See [Installer settings](#installer-settings) for a full list of options.

### Troubleshooting

Having problems with the installer?
Consult our [troubleshooting guide](./docs/troubleshooting.md) to see if your problem is covered.

### Upgrading Nix

You can upgrade Nix to [our currently recommended version of Nix][recommended-nix] by running:

```shell
sudo -i nix upgrade-nix
```

Alternatively, you can [uninstall](#uninstalling) and [reinstall](#install-nix) with a different version of the Determinate Nix Installer.

### Uninstalling

You can remove Nix installed by Determinate Nix Installer by running:

```shell
/nix/nix-installer uninstall
```

### As a Github Action

You can install Nix on [GitHub Actions][actions] using [`nix-installer-action`][nix-installer-action].
Here's an example configuration:

```yaml
on:
  pull_request:
  push:
    branches: [main]

jobs:
  build:
    name: Build
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - name: Install Nix
        uses: DeterminateSystems/nix-installer-action@main
      - name: Run `nix build`
        run: nix build .
```

### On GitLab

[GitLab CI][gitlab-ci] runners are typically [Docker] based and run as the `root` user.
This means that `systemd` is not present, so you need to pass the `--init none` option to the Linux planner.

On the default [GitLab] runners, you can install Nix using this configuration:

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

> [!WARNING]
> When `--init none` is used, _only_ `root` or users who can elevate to `root` privileges can run Nix:
>
> ```shell
> sudo -i nix run nixpkgs#hello
> ```

If you don't use [systemd], you can still install Nix by explicitly specifying the `linux` plan and `--init none`:

```shell
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install linux --init none
```

### In a container

In Docker/Podman containers or WSL2 instances where an init (like `systemd`) is not present, pass `--init none`.

For containers (without an init):

> [!WARNING]
> When `--init none` is used, _only_ `root` or users who can elevate to `root` privileges can run Nix:
>
> ```shell
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

```shell
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

```shell
podman build -t ubuntu-systemd-with-nix .
IMAGE=$(podman create ubuntu-systemd-with-nix)
CONTAINER=$(podman start $IMAGE)
podman exec -ti $CONTAINER /bin/bash
podman rm -f $CONTAINER
podman rmi $IMAGE
```

With some container tools, such as [Docker], you can omit `sandbox = false`.
Omitting this will negatively impact compatibility with container tools like [Podman].

### In WSL2

We **strongly recommend** first [enabling systemd][enabling-systemd] and then installing Nix as normal:

```shell
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

> [!WARNING]
> When `--init none` is used, _only_ `root` or users who can elevate to `root` privileges can run Nix:
>
> ```shell
> sudo -i nix run nixpkgs#hello
> ```

```shell
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install linux --init none
```

### Skip confirmation

If you'd like to bypass the confirmation step, you can apply the `--no-confirm` flag:

```shell
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install --no-confirm
```

This is especially useful when using the installer in non-interactive scripts.

## Quirks

While Determinate Nix Installer tries to provide a comprehensive and unquirky experience, there are unfortunately some issues which may require manual intervention or operator choices.

### Using MacOS after removing `nix` while `nix-darwin` was still installed, network requests fail

If `nix` was previously uninstalled without uninstalling `nix-darwin` first, users may experience errors similar to this:

```shell
nix shell nixpkgs#curl

error: unable to download 'https://cache.nixos.org/g8bqlgmpa4yg601w561qy2n576i6g0vh.narinfo': Problem with the SSL CA cert (path? access rights?) (77)
```

This occurs because `nix-darwin` provisions an `org.nixos.activate-system` service which remains after Nix is uninstalled.
The `org.nixos.activate-system` service in this state interacts with the newly installed Nix and changes the SSL certificates it uses to be a broken symlink.

```shell
ls -lah /etc/ssl/certs

total 0
drwxr-xr-x  3 root  wheel    96B Oct 17 08:26 .
drwxr-xr-x  6 root  wheel   192B Sep 16 06:28 ..
lrwxr-xr-x  1 root  wheel    41B Oct 17 08:26 ca-certificates.crt -> /etc/static/ssl/certs/ca-certificates.crt
```

The problem is compounded by the matter that the [`nix-darwin` uninstaller](https://github.com/LnL7/nix-darwin#uninstalling) will not work after uninstalling Nix, since it uses Nix and requires network connectivity.

It's possible to resolve this situation by removing the `org.nixos.activate-system` service and the `ca-certificates`:

```shell
sudo rm /Library/LaunchDaemons/org.nixos.activate-system.plist
sudo launchctl bootout system/org.nixos.activate-system
/nix/nix-installer uninstall
sudo rm /etc/ssl/certs/ca-certificates.crt
```

Run the installer again and it should work.

Up-to-date versions of `nix-installer` will refuse to uninstall until `nix-darwin` is uninstalled first, helping to mitigate this problem.

## Building a binary

Since you'll be using `nix-installer` to install Nix on systems without Nix, the default build is a static binary.

To build a portable Linux binary on a system with Nix:

```shell
# to build a local copy
nix build -L ".#nix-installer-static"
# to build the remote main development branch
nix build -L "github:determinatesystems/nix-installer#nix-installer-static"
# for a specific version of the installer:
export NIX_INSTALLER_TAG="v0.6.0"
nix build -L "github:determinatesystems/nix-installer/$NIX_INSTALLER_TAG#nix-installer-static"
```

On macOS:

```shell
# to build a local copy
nix build -L ".#nix-installer"
# to build the remote main development branch
nix build -L "github:determinatesystems/nix-installer#nix-installer"
# for a specific version of the installer:
export NIX_INSTALLER_TAG="v0.6.0"
nix build -L "github:determinatesystems/nix-installer/$NIX_INSTALLER_TAG#nix-installer"
```

Then copy `result/bin/nix-installer` to the machine you wish to run it on.
You can also add `nix-installer` to a system without Nix using [cargo], as there are no system dependencies to worry about:

```shell
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

To make this build portable, pass the `--target x86_64-unknown-linux-musl` option.

> [!NOTE]
> We currently require `--cfg tokio_unstable` as we utilize [Tokio's process groups](https://docs.rs/tokio/1.24.1/tokio/process/struct.Command.html#method.process_group), which wrap stable `std` APIs, but are unstable due to it requiring an MSRV bump.

## As a Rust library

> [!WARNING]
> Using Determinate Nix Installer as a [Rust] library is still experimental.
> This feature is likely to be removed in the future without an advocate.
> If you're using this, please let us know and we can provide a path to stabilization.

Add the [`nix-installer` library][lib] to your dependencies:

```shell
cargo add nix-installer
```

If you're building a CLI, check out the `cli` feature flag for [`clap`][clap] integration.

You'll also need to edit your `.cargo/config.toml` to use `tokio_unstable` as we utilize [Tokio's process groups][process-groups], which wrap stable `std` APIs, but are unstable due to it requiring an MSRV bump:

```toml
# .cargo/config.toml
[build]
rustflags=["--cfg", "tokio_unstable"]
```

You'll also need to set the `NIX_INSTALLER_TARBALL_PATH` environment variable to point to a target-appropriate Nix installation tarball, like nix-2.21.2-aarch64-darwin.tar.xz.
The contents are embedded in the resulting binary instead of downloaded at installation time.

Then it's possible to review the [documentation](https://docs.rs/nix-installer/latest/nix_installer/):

```shell
cargo doc --open -p nix-installer
```

Documentation is also available via `nix` build:

```shell
nix build github:DeterminateSystems/nix-installer#nix-installer.doc
firefox result-doc/nix-installer/index.html
```

## Accessing other versions

You can pin to a specific version of the Determinate Nix Installer by modifying the download URL.
Here's an example:

```shell
VERSION="v0.6.0"
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix/tag/${VERSION} | sh -s -- install
```

To discover which versions are available, or download the binaries for any release, check the [Github Releases][releases].

You can download and use these releases directly.
Here's an example:

```shell
VERSION="v0.6.0"
ARCH="aarch64-linux"
curl -sSf -L https://github.com/DeterminateSystems/nix-installer/releases/download/${VERSION}/nix-installer-${ARCH} -o nix-installer
./nix-installer install
```

Each installer version has an [associated supported nix version](src/settings.rs)&mdash;if you pin the installer version, you'll also indirectly pin to the associated nix version.

You can also override the Nix version using `--nix-package-url` or `NIX_INSTALLER_NIX_PACKAGE_URL=` but doing this is not recommended since we haven't tested that combination.
Here are some example Nix package URLs, including the Nix version, OS, and architecture:

- https://releases.nixos.org/nix/nix-2.18.1/nix-2.18.1-x86_64-linux.tar.xz
- https://releases.nixos.org/nix/nix-2.18.1/nix-2.18.1-aarch64-darwin.tar.xz

## Installation differences

Differing from the upstream [Nix][upsteam-nix] installer scripts:

- In `nix.conf`:
  - the `nix-command` and `flakes` features are enabled
  - `bash-prompt-prefix` is set
  - `auto-optimise-store` is set to `true` (On Linux only)
  * `always-allow-substitutes` is set to `true`
  * `extra-nix-path` is set to `nixpkgs=flake:nixpkgs`
  * `max-jobs` is set to `auto`
  * `upgrade-nix-store-path-url` is set to `https://install.determinate.systems/nix-upgrade/stable/universal`, to prevent unintentional downgrades.
- an installation receipt (for uninstalling) is stored at `/nix/receipt.json` as well as a copy of the install binary at `/nix/nix-installer`
- `nix-channel --update` is not run, `~/.nix-channels` is not provisioned
- `ssl-cert-file` is set in `/etc/nix/nix.conf` if the `ssl-cert-file` argument is used.

## Motivations

The existing upstream scripts do a good job but they are difficult to maintain.

Subtle differences in the shell implementations and tool used in the scripts make it difficult to make meaningful changes to the installer.

Determinate Nix installer has numerous advantages:

- survives macOS upgrades
- keeping an installation receipt for easy uninstallation
- offering users a chance to review an accurate, calculated install plan
- having 'planners' which can create appropriate install plans for complicated targets
- offering users with a failing install the chance to do a best-effort revert
- improving performance by maximizing parallel operations
- supporting a expanded test suite including 'curing' cases
- supporting SELinux and OSTree-based distributions without asking users to make compromises
- operating as a single, static binary with external dependencies such as `openssl`, only calling existing system tools (like `useradd`) where necessary
- As a MacOS remote build target, ensures `nix` is not absent from path

It has been wonderful to collaborate with other participants in the Nix Installer Working Group and members of the broader community. The working group maintains a [foundation owned fork of the installer](https://github.com/nixos/experimental-nix-installer/).

## Installer settings

The Determinate Nix Installer provides a variety of configuration settings, some [general](#general-settings) and some on a per-command basis.
All settings are available via flags or via `NIX_INSTALLER_*` environment variables.

### General settings

These settings are available for all commands.

| Flag(s)            | Description                                                               | Default (if any) | Environment variable           |
| ------------------ | ------------------------------------------------------------------------- | ---------------- | ------------------------------ |
| `--log-directives` | Tracing directives delimited by comma                                     |                  | `NIX_INSTALLER_LOG_DIRECTIVES` |
| `--logger`         | Which logger to use (options are `compact`, `full`, `pretty`, and `json`) | `compact`        | `NIX_INSTALLER_LOGGER`         |
| `--verbose`        | Enable debug logs, (`-vv` for trace)                                      | `false`          | `NIX_INSTALLER_VERBOSITY`      |

### Installation (`nix-installer install`)

| Flag(s)                    | Description                                                                                        | Default (if any)                                     | Environment variable                   |
| -------------------------- | -------------------------------------------------------------------------------------------------- | ---------------------------------------------------- | -------------------------------------- |
| `--determinate`            | Installs [Determinate]                                                                             | `NIX_INSTALLER_DETERMINATE`                          |
| `--diagnostic-attribution` | Relate the install diagnostic to a specific value                                                  |                                                      | `NIX_INSTALLER_DIAGNOSTIC_ATTRIBUTION` |
| `--diagnostic-endpoint`    | The URL or file path for an installation diagnostic to be sent                                     | `https://install.determinate.systems/nix/diagnostic` | `NIX_INSTALLER_DIAGNOSTIC_ENDPOINT`    |
| `--explain`                | Provide an explanation of the changes the installation process will make to your system            | `false`                                              | `NIX_INSTALLER_EXPLAIN`                |
| `--extra-conf`             | Extra configuration lines for `/etc/nix.conf`                                                      |                                                      | `NIX_INSTALLER_EXTRA_CONF`             |
| `--force`                  | If `nix-installer` should forcibly recreate files it finds existing                                | `false`                                              | `NIX_INSTALLER_FORCE`                  |
| `--init`                   | Which init system to configure (if `--init none` Nix will be root-only)                            | `launchd` (macOS), `systemd` (Linux)                 | `NIX_INSTALLER_INIT`                   |
| `--nix-build-group-id`     | The Nix build group GID                                                                            | `350` (macOS), `30000` (Linux)                       | `NIX_INSTALLER_NIX_BUILD_GROUP_ID`     |
| `--nix-build-group-name`   | The Nix build group name                                                                           | `nixbld`                                             | `NIX_INSTALLER_NIX_BUILD_GROUP_NAME`   |
| `--nix-build-user-count`   | The number of build users to create                                                                | `32`                                                 | `NIX_INSTALLER_NIX_BUILD_USER_COUNT`   |
| `--nix-build-user-id-base` | The Nix build user base UID (ascending) (NOTE: the first UID will be this base + 1)                | `350` (macOS), `30000` (Linux)                       | `NIX_INSTALLER_NIX_BUILD_USER_ID_BASE` |
| `--nix-build-user-prefix`  | The Nix build user prefix (user numbers will be postfixed)                                         | `_nixbld` (macOS), `nixbld` (Linux)                  | `NIX_INSTALLER_NIX_BUILD_USER_PREFIX`  |
| `--nix-package-url`        | The Nix package URL                                                                                |                                                      | `NIX_INSTALLER_NIX_PACKAGE_URL`        |
| `--no-confirm`             | Run installation without requiring explicit user confirmation                                      | `false`                                              | `NIX_INSTALLER_NO_CONFIRM`             |
| `--no-modify-profile`      | Modify the user profile to automatically load Nix.                                                 | `true`                                               | `NIX_INSTALLER_MODIFY_PROFILE`         |
| `--proxy`                  | The proxy to use (if any); valid proxy bases are `https://$URL`, `http://$URL` and `socks5://$URL` |                                                      | `NIX_INSTALLER_PROXY`                  |
| `--ssl-cert-file`          | An SSL cert to use (if any); used for fetching Nix and sets `ssl-cert-file` in `/etc/nix/nix.conf` |                                                      | `NIX_INSTALLER_SSL_CERT_FILE`          |
| `--no-start-daemon`        | Start the daemon (if not `--init none`)                                                            | `true`                                               | `NIX_INSTALLER_START_DAEMON`           |

You can also specify a planner with the first argument:

```shell
nix-installer install <plan>
```

Alternatively, you can use the `NIX_INSTALLER_PLAN` environment variable:

```shell
NIX_INSTALLER_PLAN=<plan> nix-installer install
```

### Uninstalling (`nix-installer uninstall`)

| Flag(s)        | Description                                                                             | Default (if any) | Environment variable       |
| -------------- | --------------------------------------------------------------------------------------- | ---------------- | -------------------------- |
| `--explain`    | Provide an explanation of the changes the installation process will make to your system | `false`          | `NIX_INSTALLER_EXPLAIN`    |
| `--no-confirm` | Run installation without requiring explicit user confirmation                           | `false`          | `NIX_INSTALLER_NO_CONFIRM` |

You can also specify an installation receipt as the first argument (the default is `/nix/receipt.json`):

```shell
nix-installer uninstall /path/to/receipt.json
```

### Planning (`nix-installer plan`)

| Flag(s)      | Description                                        | Default (if any) | Environment variable          |
| ------------ | -------------------------------------------------- | ---------------- | ----------------------------- |
| `--out-file` | Where to write the generated plan (in JSON format) | `/dev/stdout`    | `NIX_INSTALLER_PLAN_OUT_FILE` |

### Repairing (`nix-installer repair`)

| Flag(s)        | Description                                                   | Default (if any) | Environment variable       |
| -------------- | ------------------------------------------------------------- | ---------------- | -------------------------- |
| `--no-confirm` | Run installation without requiring explicit user confirmation | `false`          | `NIX_INSTALLER_NO_CONFIRM` |

### Self-test (`nix-installer self-test`)

`nix-installer self-test` only takes [general settings](#general-settings).

## Diagnostics

The goal of the Determinate Nix Installer is to successfully and correctly install Nix.
The `curl | sh` pipeline and the installer collects a little bit of diagnostic information to help us make that true.

Here is a table of the [diagnostic data we collect][diagnosticdata]:

| Field                 | Use                                                                                                                                 |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `version`             | The version of the Determinate Nix Installer.                                                                                       |
| `planner`             | The method of installing Nix (`linux`, `macos`, `steam-deck`)                                                                       |
| `configured_settings` | The names of planner settings which were changed from their default. Does _not_ include the values.                                 |
| `os_name`             | The running operating system.                                                                                                       |
| `os_version`          | The version of the operating system.                                                                                                |
| `triple`              | The architecture/operating system/binary format of your system.                                                                     |
| `is_ci`               | Whether the installer is being used in CI (e.g. GitHub Actions).                                                                    |
| `action`              | Either `Install` or `Uninstall`.                                                                                                    |
| `status`              | One of `Success`, `Failure`, `Pending`, or `Cancelled`.                                                                             |
| `attribution`         | Optionally defined by the user, associate the diagnostics of this run to the provided value.                                        |
| `failure_chain`       | A high level description of what the failure was, if any. For example: `Command("diskutil")` if the command `diskutil list` failed. |

To disable diagnostic reporting, set the diagnostics URL to an empty string by passing `--diagnostic-endpoint=""` or setting `NIX_INSTALLER_DIAGNOSTIC_ENDPOINT=""`.

You can read the full privacy policy for [Determinate Systems][detsys], the creators of the Determinate Nix Installer, [here][privacy].

[actions]: https://github.com/features/actions
[cargo]: https://doc.rust-lang.org/cargo
[clap]: https://clap.rs
[determinate]: https://docs.determinate.systems
[detsys]: https://determinate.systems
[docker]: https://docker.com
[diagnosticdata]: https://github.com/DeterminateSystems/nix-installer/blob/f9f927840d532b71f41670382a30cfcbea2d8a35/src/diagnostics.rs#L29-L43
[enabling-systemd]: https://devblogs.microsoft.com/commandline/systemd-support-is-now-available-in-wsl/#how-can-you-get-systemd-on-your-machine
[gitlab]: https://gitlab.com
[gitlab-ci]: https://docs.gitlab.com/ee/ci
[lib]: https://docs.rs/nix-installer
[nix]: https://nixos.org
[nix-installer-action]: https://github.com/DeterminateSystems/nix-installer-action
[nixgl]: https://github.com/guibou/nixGL
[podman]: https://podman.io
[privacy]: https://determinate.systems/policies/privacy
[process-groups]: https://docs.rs/tokio/1.24.1/tokio/process/struct.Command.html#method.process_group
[recommended-nix]: https://github.com/DeterminateSystems/nix/releases/latest
[releases]: https://github.com/DeterminateSystems/nix-installer/releases
[rust]: https://rust-lang.org
[steam-deck]: https://store.steampowered.com/steamdeck
[systemd]: https://systemd.io
[upstream-nix]: https://github.com/NixOS/nix
[wsl]: https://learn.microsoft.com/en-us/windows/wsl/about
[wslg]: https://github.com/microsoft/wslg
