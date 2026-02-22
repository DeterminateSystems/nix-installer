{
  description = "The Determinate Nix Installer";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1.793735";

    crane.url = "github:ipetkov/crane/v0.20.0";

    nix = {
      url = "https://flakehub.com/f/DeterminateSystems/nix-src/*";
      # Omitting `inputs.nixpkgs.follows = "nixpkgs";` on purpose
    };

    determinate = {
      url = "https://flakehub.com/f/DeterminateSystems/determinate/*";

      # We set the overrides below so the flake.lock has many fewer nodes.
      #
      # The `determinate` input is used to access the builds of `determinate-nixd`.
      # Below, we access the `packages` outputs, which download static builds of `determinate-nixd` and makes them executable.
      # The way we consume the determinate flake means the `nix` and `nixpkgs` inputs are not meaningfully used.
      # This means `follows` won't cause surprisingly extensive rebuilds, just trivial `chmod +x` rebuilds.
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.nix.follows = "nix";
    };
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
      nix_tarball_url_prefix = "https://releases.nixos.org/nix/nix-2.33.3/nix-2.33.3-";
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];
      systemsSupportedByDeterminateNixd = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];

      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: (forSystem system f));

      forSystem = system: f: f rec {
        inherit system;
        pkgs = import nixpkgs { inherit system; overlays = [ self.overlays.default ]; };
        lib = pkgs.lib;
      };

      nixTarballs = forAllSystems ({ system, ... }:
        inputs.nix.tarballs_direct.${system}
          or "${inputs.nix.packages."${system}".binaryTarball}/nix-${inputs.nix.packages."${system}".default.version}-${system}.tar.xz");

      optionalPathToDeterminateNixd = system: if builtins.elem system systemsSupportedByDeterminateNixd then "${inputs.determinate.packages.${system}.default}/bin/determinate-nixd" else null;

      installerPackage = { pkgs, stdenv, buildPackages }:
        let
          craneLib = crane.mkLib pkgs;
          sharedAttrs = {
            src = builtins.path {
              name = "nix-installer-source";
              path = self;
              filter = (path: type: baseNameOf path != "nix" && baseNameOf path != ".github");
            };

            # Required to link build scripts.
            depsBuildBuild = [ buildPackages.stdenv.cc ];

            env = {
              # For whatever reason, these don’t seem to get set
              # automatically when using crane.
              #
              # Possibly related: <https://github.com/NixOS/nixpkgs/pull/369424>
              "CC_${stdenv.hostPlatform.rust.cargoEnvVarTarget}" = "${stdenv.cc.targetPrefix}cc";
              "CXX_${stdenv.hostPlatform.rust.cargoEnvVarTarget}" = "${stdenv.cc.targetPrefix}c++";
              "CARGO_TARGET_${stdenv.hostPlatform.rust.cargoEnvVarTarget}_LINKER" = "${stdenv.cc.targetPrefix}cc";
              CARGO_BUILD_TARGET = stdenv.hostPlatform.rust.rustcTarget;
            };
          };
        in
        craneLib.buildPackage (sharedAttrs // {
          cargoArtifacts = craneLib.buildDepsOnly sharedAttrs;

          cargoTestExtraArgs = "--all";

          postInstall = ''
            cp nix-installer.sh $out/bin/nix-installer.sh
          '';

          env = sharedAttrs.env // {
            RUSTFLAGS = "--cfg tokio_unstable";
            NIX_TARBALL_URL = "${nix_tarball_url_prefix}${pkgs.stdenv.hostPlatform.system}.tar.xz";
            DETERMINATE_NIX_TARBALL_PATH = nixTarballs.${stdenv.hostPlatform.system};
            DETERMINATE_NIXD_BINARY_PATH = optionalPathToDeterminateNixd stdenv.hostPlatform.system;
          };
        });
    in
    {
      overlays.default = final: prev: {
        nix-installer = final.callPackage installerPackage { };
        nix-installer-static = final.pkgsStatic.callPackage installerPackage { };
      };

      devShells = forAllSystems ({ system, pkgs, ... }:
        let
          check = import ./nix/check.nix { inherit pkgs; };
        in
        {
          default = pkgs.mkShell {
            name = "nix-install-shell";

            RUST_SRC_PATH = "${pkgs.rustPlatform.rustcSrc}/library";
            NIX_TARBALL_URL = "${nix_tarball_url_prefix}${pkgs.stdenv.hostPlatform.system}.tar.xz";
            DETERMINATE_NIX_TARBALL_PATH = nixTarballs.${system};
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
              toml-cli
            ]
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
          inherit (pkgs) nix-installer nix-installer-static;
          default = pkgs.nix-installer-static;
        } // nixpkgs.lib.optionalAttrs (pkgs.stdenv.isDarwin) {
          determinate-nixd = pkgs.runCommand "determinate-nixd-link" { } ''
            ln -s ${optionalPathToDeterminateNixd system} $out
          '';
        });

      hydraJobs = {
        vm-test = import ./nix/tests/vm-test {
          inherit forSystem;
          inherit (nixpkgs) lib;

          binaryTarball = nix.hydraJobs.binaryTarball;
        };
        container-test = import ./nix/tests/container-test {
          inherit forSystem;

          binaryTarball = nix.hydraJobs.binaryTarball;
        };
      };
    };
}
