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

You'll also need to set the `NIX_INSTALLER_TARBALL_PATH` environment variable to point to a target-appropriate Nix installation tarball, like nix-2.31.1-aarch64-darwin.tar.xz.
The contents are embedded in the resulting binary instead of downloaded at installation time.

Then it's possible to review the [documentation]:

```shell
cargo doc --open -p nix-installer
```

Documentation is also available via `nix build`:

```shell
nix build github:DeterminateSystems/nix-installer#nix-installer.doc
firefox result-doc/nix-installer/index.html
```

[clap]: https://clap.rs
[documentation]: https://docs.rs/nix-installer/latest/nix_installer
[lib]: https://docs.rs/nix-installer
[process-groups]: https://docs.rs/tokio/latest/tokio/process/struct.Command.html#method.process_group
[rust]: https://rust-lang.com
