{
  description = "The Determinate Nix Installer";

  inputs = {
    # The very next version and beyond we get SIGBUS on uninstall
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/=0.1.698755";

    crane.url = "github:ipetkov/crane/v0.20.0";

    nix = {
      url = "https://flakehub.com/f/DeterminateSystems/nix/=2.25.3.tar.gz";
      # Omitting `inputs.nixpkgs.follows = "nixpkgs";` on purpose
    };

    determinate = {
      url = "https://flakehub.com/f/DeterminateSystems/determinate/0.1.tar.gz";

      # We set the overrides below so the flake.lock has many fewer nodes.
      #
      # The `determinate` input is used to access the builds of `determinate-nixd`.
      # Below, we access the `packages` outputs, which download static builds of `determinate-nixd` and makes them executable.
      # The way we consume the determinate flake means the `nix` and `nixpkgs` inputs are not meaningfully used.
      # This means `follows` won't cause surprisingly extensive rebuilds, just trivial `chmod +x` rebuilds.
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.nix.follows = "nix";
    };

    flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.0.0.tar.gz";
  };

  outputs =
    { self
    , nixpkgs
    , crane
    , nix
    , determinate
    , ...
    } @ inputs:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      systemsSupportedByDeterminateNixd = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: (forSystem system f));

      forSystem = system: f: f rec {
        inherit system;
        pkgs = import nixpkgs { inherit system; overlays = [ self.overlays.default ]; };
        lib = pkgs.lib;
      };

      nixTarballs = forAllSystems ({ system, ... }:
        inputs.nix.tarballs_direct.${system}
          or "${inputs.nix.checks."${system}".binaryTarball}/nix-${inputs.nix.packages."${system}".default.version}-${system}.tar.xz");

      optionalPathToDeterminateNixd = system: if builtins.elem system systemsSupportedByDeterminateNixd then "${inputs.determinate.packages.${system}.default}/bin/determinate-nixd" else null;
    in
    {
      overlays.default = final: prev:
        let
          craneLib = crane.mkLib final;
          sharedAttrs = {
            pname = "nix-installer";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
            src = builtins.path {
              name = "nix-installer-source";
              path = self;
              filter = (path: type: baseNameOf path != "nix" && baseNameOf path != ".github");
            };

            nativeBuildInputs = with final; [ ];
            buildInputs = with final; [ ] ++ lib.optionals (final.stdenv.isDarwin) (with final.darwin.apple_sdk.frameworks; [
              SystemConfiguration
              final.darwin.libiconv
            ]);

            RUSTFLAGS = "--cfg tokio_unstable";
            cargoTestExtraArgs = "--all";

            NIX_INSTALLER_TARBALL_PATH = nixTarballs.${final.stdenv.system};
            DETERMINATE_NIXD_BINARY_PATH = optionalPathToDeterminateNixd final.stdenv.system;

          };
          installerAttrs = sharedAttrs // {
            cargoArtifacts = craneLib.buildDepsOnly sharedAttrs;

            preBuild = ''
              # logRun "cargo clippy --all-targets --all-features -- -D warnings"
            '';

            postInstall = ''
              cp nix-installer.sh $out/bin/nix-installer.sh
            '';
          };
        in
        {
          nix-installer = craneLib.buildPackage installerAttrs;
        } // nixpkgs.lib.optionalAttrs (prev.stdenv.system == "x86_64-linux") {
          nix-installer-static = craneLib.buildPackage
            (installerAttrs // {
              CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
            });
        } // nixpkgs.lib.optionalAttrs (prev.stdenv.system == "aarch64-linux") {
          nix-installer-static = craneLib.buildPackage
            (installerAttrs // {
              CARGO_BUILD_TARGET = "aarch64-unknown-linux-musl";
            });
        };


      devShells = forAllSystems ({ system, pkgs, ... }:
        let
          check = import ./nix/check.nix { inherit pkgs; };
        in
        {
          default = pkgs.mkShell {
            name = "nix-install-shell";

            RUST_SRC_PATH = "${pkgs.rustPlatform.rustcSrc}/library";
            NIX_INSTALLER_TARBALL_PATH = nixTarballs.${system};
            DETERMINATE_NIXD_BINARY_PATH = optionalPathToDeterminateNixd system;

            nativeBuildInputs = with pkgs; [ ];
            buildInputs = with pkgs; [
              rustc
              cargo
              clippy
              rustfmt
              shellcheck
              rust-analyzer
              cargo-outdated
              cacert
              # cargo-audit # NOTE(cole-h): build currently broken because of time dependency and Rust 1.80
              cargo-watch
              nixpkgs-fmt
              check.check-rustfmt
              check.check-spelling
              check.check-nixpkgs-fmt
              check.check-editorconfig
              check.check-semver
              check.check-clippy
              editorconfig-checker
            ]
            ++ lib.optionals (pkgs.stdenv.isDarwin) (with pkgs; [
              libiconv
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.SystemConfiguration
            ])
            ++ lib.optionals (pkgs.stdenv.isLinux) (with pkgs; [
              checkpolicy
              semodule-utils
              /* users are expected to have a system docker, too */
            ]);
          };
        });

      checks = forAllSystems ({ system, pkgs, ... }:
        let
          check = import ./nix/check.nix { inherit pkgs; };
        in
        {
          check-rustfmt = pkgs.runCommand "check-rustfmt" { buildInputs = [ check.check-rustfmt ]; } ''
            cd ${./.}
            check-rustfmt
            touch $out
          '';
          check-spelling = pkgs.runCommand "check-spelling" { buildInputs = [ check.check-spelling ]; } ''
            cd ${./.}
            check-spelling
            touch $out
          '';
          check-nixpkgs-fmt = pkgs.runCommand "check-nixpkgs-fmt" { buildInputs = [ check.check-nixpkgs-fmt ]; } ''
            cd ${./.}
            check-nixpkgs-fmt
            touch $out
          '';
          check-editorconfig = pkgs.runCommand "check-editorconfig" { buildInputs = [ pkgs.git check.check-editorconfig ]; } ''
            cd ${./.}
            check-editorconfig
            touch $out
          '';
        });

      packages = forAllSystems ({ system, pkgs, ... }:
        {
          inherit (pkgs) nix-installer;
        } // nixpkgs.lib.optionalAttrs (system == "x86_64-linux") {
          inherit (pkgs) nix-installer-static;
          default = pkgs.nix-installer-static;
        } // nixpkgs.lib.optionalAttrs (system == "aarch64-linux") {
          inherit (pkgs) nix-installer-static;
          default = pkgs.nix-installer-static;
        } // nixpkgs.lib.optionalAttrs (pkgs.stdenv.isDarwin) {
          default = pkgs.nix-installer;

          determinate-nixd = pkgs.runCommand "determinate-nixd-link" { } ''
            ln -s ${optionalPathToDeterminateNixd system} $out
          '';
        });

      hydraJobs = {
        vm-test = import ./nix/tests/vm-test {
          inherit forSystem;
          inherit (nixpkgs) lib;

          binaryTarball = nix.tarballs_indirect;
        };
        container-test = import ./nix/tests/container-test {
          inherit forSystem;

          binaryTarball = nix.tarballs_indirect;
        };
      };
    };
}
