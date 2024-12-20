{
  description = "Experimental Nix Installer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/63c3a29ca82437c87573e4c6919b09a24ea61b0f";

    fenix = {
      url = "github:nix-community/fenix/73124e1356bde9411b163d636b39fe4804b7ca45";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nix = {
      url = "github:NixOS/nix/2.24.9";
      # Omitting `inputs.nixpkgs.follows = "nixpkgs";` on purpose
    };
    # We don't use this, so let's save download/update time
    # determinate = {
    #   url = "https://flakehub.com/f/DeterminateSystems/determinate/0.1.tar.gz";

    #   # We set the overrides below so the flake.lock has many fewer nodes.
    #   #
    #   # The `determinate` input is used to access the builds of `determinate-nixd`.
    #   # Below, we access the `packages` outputs, which download static builds of `determinate-nixd` and makes them executable.
    #   # The way we consume the determinate flake means the `nix` and `nixpkgs` inputs are not meaningfully used.
    #   # This means `follows` won't cause surprisingly extensive rebuilds, just trivial `chmod +x` rebuilds.
    #   inputs.nixpkgs.follows = "nixpkgs";
    #   inputs.nix.follows = "nix";
    # };

    flake-compat.url = "github:edolstra/flake-compat/v1.0.0";
  };

  outputs =
    { self
    , nixpkgs
    , fenix
    , naersk
    , nix
      # , determinate
    , ...
    } @ inputs:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      systemsSupportedByDeterminateNixd = [  ]; # avoid refs to detsys nixd for now

      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: (forSystem system f));

      forSystem = system: f: f rec {
        inherit system;
        pkgs = import nixpkgs { inherit system; overlays = [ self.overlays.default ]; };
        lib = pkgs.lib;
      };

      fenixToolchain = system: with fenix.packages.${system};
        combine ([
          stable.clippy
          stable.rustc
          stable.cargo
          stable.rustfmt
          stable.rust-src
        ] ++ nixpkgs.lib.optionals (system == "x86_64-linux") [
          targets.x86_64-unknown-linux-musl.stable.rust-std
        ] ++ nixpkgs.lib.optionals (system == "aarch64-linux") [
          targets.aarch64-unknown-linux-musl.stable.rust-std
        ]);

      nixTarballs = forAllSystems ({ system, ... }:
        inputs.nix.tarballs_direct.${system}
          or "${inputs.nix.checks."${system}".binaryTarball}/nix-${inputs.nix.packages."${system}".default.version}-${system}.tar.xz");

      optionalPathToDeterminateNixd = system: if builtins.elem system systemsSupportedByDeterminateNixd then "${inputs.determinate.packages.${system}.default}/bin/determinate-nixd" else null;
    in
    {
      overlays.default = final: prev:
        let
          toolchain = fenixToolchain final.stdenv.system;
          naerskLib = final.callPackage naersk {
            cargo = toolchain;
            rustc = toolchain;
          };
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
            ]);

            copyBins = true;
            copyDocsToSeparateOutput = true;

            doCheck = false;
            doDoc = true;
            doDocFail = true;
            RUSTFLAGS = "--cfg tokio_unstable";
            cargoTestOptions = f: f ++ [ "--all" ];

            NIX_INSTALLER_TARBALL_PATH = nixTarballs.${final.stdenv.system};
            DETERMINATE_NIXD_BINARY_PATH = optionalPathToDeterminateNixd final.stdenv.system;

            override = { preBuild ? "", ... }: {
              preBuild = preBuild + ''
                # logRun "cargo clippy --all-targets --all-features -- -D warnings"
              '';
            };
            postInstall = ''
              cp nix-installer.sh $out/bin/nix-installer.sh
            '';
          };
        in
        rec {
          # NOTE(cole-h): fixes build -- nixpkgs updated libsepol to 3.7 but didn't update
          # checkpolicy to 3.7, checkpolicy links against libsepol, and libsepol 3.7 changed
          # something in the API so checkpolicy 3.6 failed to build against libsepol 3.7
          # Can be removed once https://github.com/NixOS/nixpkgs/pull/335146 merges.
          checkpolicy = prev.checkpolicy.overrideAttrs ({ ... }: rec {
            version = "3.7";

            src = final.fetchurl {
              url = "https://github.com/SELinuxProject/selinux/releases/download/${version}/checkpolicy-${version}.tar.gz";
              sha256 = "sha256-/T4ZJUd9SZRtERaThmGvRMH4bw1oFGb9nwLqoGACoH8=";
            };
          });

          nix-installer = naerskLib.buildPackage sharedAttrs;
        } // nixpkgs.lib.optionalAttrs (prev.stdenv.system == "x86_64-linux") rec {
          default = nix-installer-static;
          nix-installer-static = naerskLib.buildPackage
            (sharedAttrs // {
              CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
            });
        } // nixpkgs.lib.optionalAttrs (prev.stdenv.system == "aarch64-linux") rec {
          default = nix-installer-static;
          nix-installer-static = naerskLib.buildPackage
            (sharedAttrs // {
              CARGO_BUILD_TARGET = "aarch64-unknown-linux-musl";
            });
        };


      devShells = forAllSystems ({ system, pkgs, ... }:
        let
          toolchain = fenixToolchain system;
          check = import ./nix/check.nix { inherit pkgs toolchain; };
        in
        {
          default = pkgs.mkShell {
            name = "nix-install-shell";

            RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
            NIX_INSTALLER_TARBALL_PATH = nixTarballs.${system};
            DETERMINATE_NIXD_BINARY_PATH = optionalPathToDeterminateNixd system;

            nativeBuildInputs = with pkgs; [ ];
            buildInputs = with pkgs; [
              toolchain
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
          toolchain = fenixToolchain system;
          check = import ./nix/check.nix { inherit pkgs toolchain; };
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
        });

      hydraJobs = {
        build = forAllSystems ({ system, pkgs, ... }: self.packages.${system}.default);
        # vm-test = import ./nix/tests/vm-test {
        #   inherit forSystem;
        #   inherit (nixpkgs) lib;

        #   binaryTarball = nix.tarballs_indirect;
        # };
        # container-test = import ./nix/tests/container-test {
        #   inherit forSystem;

        #   binaryTarball = nix.tarballs_indirect;
        # };
      };
    };
}
