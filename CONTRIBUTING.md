# Contributing to `experimental-nix-installer`

Most contributions should be made upstream; see the
[upstream contributing guide](https://github.com/DeterminateSystems/nix-installer/blob/main/CONTRIBUTING.md)
for guidelines.

In general, bugs should also be opened in the
[upstream issue tracker](https://github.com/DeterminateSystems/nix-installer/issues).

In some cases the Determinate Systems installer has made decisions that do not
align with decisions of the official Nix installer, and in those cases, a
contribution to this repo is appropriate.

# Development

Some snippets or workflows for development.


## Direnv support

While `nix develop` should work perfectly fine for development, contributors may prefer to enable [`direnv`](https://direnv.net/) or [`nix-direnv`](https://github.com/nix-community/nix-direnv) support.

From the project folder:

```bash
direnv allow
```

If using an editor, it may be preferable to adopt an addon to enter the environment:

* [`vim`](https://github.com/direnv/direnv.vim)
* [VSCode](https://marketplace.visualstudio.com/items?itemName=mkhl.direnv)


## Testing Installs

If you're hacking on `experimental-nix-installer`, you likely already have Nix and cannot test locally.

> That's probably a good thing! You should test in a sandbox.

Automated [`qemu` tests][#qemu-vm-tests] exist and should be preferred for oneshot testing of changes.

For interactive testing, tools like [`libvirt`](https://libvirt.org/) via [`virt-manager`](https://virt-manager.org/) or [`vagrant`](https://www.vagrantup.com/) can be used to spin up machines and run experiments.

When running such interactive tests, consider creating a snapshot of the VM right before running the installer, so you can quickly roll back if something happens.

In general, it's a good idea to test on the closest you can get to the desired target environment. For example, when testing the Steam Deck planner it's a good idea to run that test in a Steam Deck VM as described in detail in the planner.


<details>
  <summary><strong>Adding a planner for specific hardware?</strong></summary>

Please include an full guide on how to create the best known virtual testing environment for that device. 

**A link is not sufficient, it may break.** Please provide a full summary of steps to take, link to any original source and give them credit if it is appropriate.

It's perfectly fine if they are manual or labor intensive, as these should be a one time thing and get snapshotted prior to running tests.

</details>

## `qemu` VM tests

For x86_64 Linux we have some additional QEMU based tests. In `nix/tests/vm-test` there exists some Nix derivations which we expose in the flake via `hydraJobs`.

These should be visible in `nix flake show`:

```
❯ nix flake show
warning: Git tree '/home/ana/git/determinatesystems/nix-installer' is dirty
git+file:///home/ana/git/determinatesystems/nix-installer
# ...
├───hydraJobs
│   └───vm-test
│       ├───all
│       │   └───x86_64-linux
│       │       └───install-default: derivation 'all'
│       ├───fedora-v36
│       │   └───x86_64-linux
│       │       └───install-default: derivation 'installer-test-fedora-v36-install-default'
│       ├───rhel-v7
│       │   └───x86_64-linux
│       │       └───install-default: derivation 'installer-test-rhel-v7-install-default'
│       ├───rhel-v8
│       │   └───x86_64-linux
│       │       └───install-default: derivation 'installer-test-rhel-v8-install-default'
│       ├───rhel-v9
│       │   └───x86_64-linux
│       │       └───install-default: derivation 'installer-test-rhel-v9-install-default'
│       └───ubuntu-v22_04
│           └───x86_64-linux
│               └───install-default: derivation 'installer-test-ubuntu-v22_04-install-default'
```

To run all of the currently supported tests:

```bash
nix build .#hydraJobs.vm-test.all.x86_64-linux.all -L
```

To run a specific distribution listed in the `nix flake show` output:

```bash
nix build .#hydraJobs.vm-test.rhel-v7.x86_64-linux.all -L -j 4
```

> You may wish to set `-j 4` to some other number. Some OS's (Ubuntu 16.04) exhibit problems rapidly updating their users/groups on a system running dozens of VMs.

For PR review, you can also test arbitrary branches or checkouts like so:

```bash
nix build github:NixOS/experimental-nix-installer/${BRANCH}#hydraJobs.vm-test.ubuntu-v22_04.x86_64-linux.install-default -L
```

<details>
  <summary><strong>Adding a distro?</strong></summary>

Notice how `rhel-v7` has a `v7`, not just `7`? That's so the test output shows correctly, as Nix will interpret the first `-\d` (eg `-7`, `-123213`) as a version, and not show it in the output. 

Using `v7` instead turns:

```
# ...
installer-test-rhel> Unpacking Vagrant box /nix/store/8maga4w267f77agb93inbg54whh5lxhn-libvirt.box...
installer-test-rhel> Vagrantfile
installer-test-rhel> box.img
installer-test-rhel> info.json
installer-test-rhel> metadata.json
installer-test-rhel> Formatting './disk.qcow2', fmt=qcow2 cluster_size=65536 extended_l2=off compression_type=zlib size=137438953472 backing_file=./box.img backing_fmt=qcow2 lazy_refcounts=off refcount_bits=16
# ...
```

Into this:

```
# ...
installer-test-rhel-v7-install-default> Unpacking Vagrant box /nix/store/8maga4w267f77agb93inbg54whh5lxhn-libvirt.box...
installer-test-rhel-v7-install-default> Vagrantfile
installer-test-rhel-v7-install-default> box.img
installer-test-rhel-v7-install-default> info.json
installer-test-rhel-v7-install-default> metadata.json
installer-test-rhel-v7-install-default> Formatting './disk.qcow2', fmt=qcow2 cluster_size=65536 extended_l2=off compression_type=zlib size=137438953472 backing_file=./box.img backing_fmt=qcow2 lazy_refcounts=off refcount_bits=16
# ...
```

</details>

## Container tests


For x86_64 Linux we have some additional container tests. In `nix/tests/container-test` there exists some Nix derivations which we expose in the flake via `hydraJobs`.

These should be visible in `nix flake show`:

```
❯ nix flake show
warning: Git tree '/home/ana/git/determinatesystems/nix-installer' is dirty
git+file:///home/ana/git/determinatesystems/nix-installer
# ...
├───hydraJobs
│   ├───container-test
│   │   ├───all
│   │   │   └───x86_64-linux
│   │   │       ├───all: derivation 'all'
│   │   │       ├───docker: derivation 'all'
│   │   │       └───podman: derivation 'all'
│   │   ├───ubuntu-v18_04
│   │   │   └───x86_64-linux
│   │   │       ├───all: derivation 'all'
│   │   │       ├───docker: derivation 'vm-test-run-container-test-ubuntu-v18_04'
│   │   │       └───podman: derivation 'vm-test-run-container-test-ubuntu-v18_04'
│   │   ├───ubuntu-v20_04
│   │   │   └───x86_64-linux
│   │   │       ├───all: derivation 'all'
│   │   │       ├───docker: derivation 'vm-test-run-container-test-ubuntu-v20_04'
│   │   │       └───podman: derivation 'vm-test-run-container-test-ubuntu-v20_04'
│   │   └───ubuntu-v22_04
│   │       └───x86_64-linux
│   │           ├───all: derivation 'all'
│   │           ├───docker: derivation 'vm-test-run-container-test-ubuntu-v22_04'
│   │           └───podman: derivation 'vm-test-run-container-test-ubuntu-v22_04'
```

To run all of the currently supported tests:


```bash
nix build .#hydraJobs.container-test.all.x86_64-linux.all -L -j 4
```

> You may wish to set `-j 4` to some other number. Some OS's (Ubuntu 16.04) exhibit problems rapidly updating their users/groups on a system running dozens of VMs.

To run a specific distribution listed in the `nix flake show` output:

```bash
nix build .#hydraJobs.container-test.ubuntu-v22_04.x86_64-linux.docker -L
```

For PR review, you can also test arbitrary branches or checkouts like so:

```bash
nix build github:NixOS/experimental-nix-installer/${BRANCH}#hydraJobs.container-test.ubuntu-v22_04.x86_64-linux.podman -L
```

<details>
  <summary><strong>Adding a distro?</strong></summary>

Notice how `ubuntu-v20_02` has a `v20`, not just `20`? That's so the test output shows correctly, as Nix will interpret the first `-\d` (eg `-20`, `-123213`) as a version, and not show it in the output. 

Using `v20` instead turns:

```
# ...
vm-test-run-container-test-ubuntu> machine # [   23.385182] dhcpcd[670]: vethae56f366: deleting address fe80::c036:c8ff:fe04:5832
vm-test-run-container-test-ubuntu> machine # this derivation will be built:
vm-test-run-container-test-ubuntu> machine #   /nix/store/9qb0l9n1gsmcyynfmndnq3qpmlvq8rln-foo.drv
vm-test-run-container-test-ubuntu> machine # [   23.424605] dhcpcd[670]: vethae56f366: removing interface
vm-test-run-container-test-ubuntu> machine # building '/nix/store/9qb0l9n1gsmcyynfmndnq3qpmlvq8rln-foo.drv'...
vm-test-run-container-test-ubuntu> machine # [   23.371066] systemd[1]: crun-buildah-buildah1810857047.scope: Deactivated successfully.
# ...
```

Into this:

```
# ...
vm-test-run-container-test-ubuntu-v18_04> machine # [   23.385182] dhcpcd[670]: vethae56f366: deleting address fe80::c036:c8ff:fe04:5832
vm-test-run-container-test-ubuntu-v20_04> machine # this derivation will be built:
vm-test-run-container-test-ubuntu-v20_04> machine #   /nix/store/9qb0l9n1gsmcyynfmndnq3qpmlvq8rln-foo.drv
vm-test-run-container-test-ubuntu-v18_04> machine # [   23.424605] dhcpcd[670]: vethae56f366: removing interface
vm-test-run-container-test-ubuntu-v20_04> machine # building '/nix/store/9qb0l9n1gsmcyynfmndnq3qpmlvq8rln-foo.drv'...
vm-test-run-container-test-ubuntu-v20_04> machine # [   23.371066] systemd[1]: crun-buildah-buildah1810857047.scope: Deactivated successfully.
# ...
```

</details>

## WSL tests

On a Windows Machine with WSL2 enabled (and updated to [support systemd](https://ubuntu.com/blog/ubuntu-wsl-enable-systemd)) you can test using WSL the scripts in `tests/windows`:

```powershell
.\tests\windows\test-wsl.ps1
.\tests\windows\test-wsl.ps1 -Systemd
```

If something breaks you may need to unregister the test WSL instance. First, look for the distro prefixed with `nix-installer-test`:

```powershell
$ wsl --list
Windows Subsystem for Linux Distributions:
Ubuntu (Default)
nix-installer-test-ubuntu-jammy
```

Then delete it:

```powershell
wsl --unregister nix-installer-test-ubuntu-jammy
```

You can also remove your `$HOME/nix-installer-wsl-tests-temp` folder whenever you wish.


# Releases


This package uses [Semantic Versioning](https://semver.org/). When determining the version number for a new release refer to Semantic Versioning for guidance. You can use the `check-semver` command alias from within the development environment to validate your changes don't break semver.

To cut a release:

* Ensure the `flake.lock`, `Cargo.lock`, and Rust dependencies are up-to-date with the following:
  + `nix flake update`
  + `cargo update`
  + `cargo outdated`
  + Make a PR for for this and let it get merged separately
* Create a release branch from `main` (`git checkout -b release-v0.0.1`)
* Remove the `-unreleased` from the `version` field in `Cargo.toml`, `flake.nix`, and the fixture JSON files
  + Release PRs should not contain any tangible code changes which require review
* Ensure the VM / container tests still pass with the following:
  + `nix flake check -L`
  + `nix build .#hydraJobs.container-test.all.x86_64-linux.all -L -j 6`
  + `nix build .#hydraJobs.vm-test.all.x86_64-linux.all -L -j 6`
* Push the branch, create a PR ("Release v0.0.1")
* Once the PR tests pass and it has been reviewed, merge it
* `git pull` on the `main` branch
* Tag the release (`git tag v0.0.1`)
* Push the tag (`git push origin v0.0.1`)
* The CI should produce artifacts via Buildkite and create a "Draft" release containing them on GitHub
  + This will take a bit, use this time to draft a changelog
* Review the draft release, test the artifacts in a VM
* Create a changelog following the format of last release
* Undraft the release
* Once you are certain the release is good, `cargo publish` it
  + **Warning:** While you can re-release Github releases, it is not possible to do the same on `crates.io`
* Create a PR bumping the version up one minor in the `Cargo.toml`, `flake.nix`, and fixture JSON files, adding `-unreleased` at the end (`v0.0.2-unreleased`)

# Who maintains `nix-installer` and why?

This fork is maintained by the community.

The upstream `nix-installer` is maintained by [Determinate Systems](https://determinate.systems/) in
an effort to explore Nix installer ideas.

Determinate Systems has no plans to monetize or relicense `nix-installer`. If your
enterprise requires a support contact in order to adopt a tool, please contact
Determinate Systems and something can be worked out.
