{
  description = "The Determinate Nix Installer";

  inputs = {
    nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0.1.0.tar.gz";

    fenix = {
      url = "https://flakehub.com/f/nix-community/fenix/0.1.1584.tar.gz";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    nix = {
      url = "https://flakehub.com/f/DeterminateSystems/nix/2.0.0";
      # Omitting `inputs.nixpkgs.follows = "nixpkgs";` on purpose
    };

    flake-compat.url = "https://flakehub.com/f/edolstra/flake-compat/1.0.0.tar.gz";
  };

  outputs =
    { self
    , nixpkgs
    , fenix
    , naersk
    , nix
    , ...
    } @ inputs:
    let
      supportedSystems = [ "i686-linux" "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

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
        ] ++ nixpkgs.lib.optionals (system == "i686-linux") [
          targets.i686-unknown-linux-musl.stable.rust-std
        ] ++ nixpkgs.lib.optionals (system == "aarch64-linux") [
          targets.aarch64-unknown-linux-musl.stable.rust-std
        ]);
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
            version = "0.17.1";
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

            doCheck = true;
            doDoc = true;
            doDocFail = true;
            RUSTFLAGS = "--cfg tokio_unstable";
            cargoTestOptions = f: f ++ [ "--all" ];

            NIX_INSTALLER_TARBALL = inputs.nix.tarballs_direct.${final.stdenv.system};

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
          nix-installer = naerskLib.buildPackage sharedAttrs;
        } // nixpkgs.lib.optionalAttrs (prev.stdenv.system == "x86_64-linux") rec {
          default = nix-installer-static;
          nix-installer-static = naerskLib.buildPackage
            (sharedAttrs // {
              CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
            });
        } // nixpkgs.lib.optionalAttrs (prev.stdenv.system == "i686-linux") rec {
          default = nix-installer-static;
          nix-installer-static = naerskLib.buildPackage
            (sharedAttrs // {
              CARGO_BUILD_TARGET = "i686-unknown-linux-musl";
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
            NIX_INSTALLER_TARBALL = inputs.nix.tarballs_direct.${system};

            nativeBuildInputs = with pkgs; [ ];
            buildInputs = with pkgs; [
              toolchain
              rust-analyzer
              cargo-outdated
              cacert
              cargo-audit
              cargo-watch
              nixpkgs-fmt
              check.check-rustfmt
              check.check-spelling
              check.check-nixpkgs-fmt
              check.check-editorconfig
              check.check-semver
              check.check-clippy
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
        } // nixpkgs.lib.optionalAttrs (system == "i686-linux") {
          inherit (pkgs) nix-installer-static;
          default = pkgs.nix-installer-static;
        } // nixpkgs.lib.optionalAttrs (system == "aarch64-linux") {
          inherit (pkgs) nix-installer-static;
          default = pkgs.nix-installer-static;
        } // nixpkgs.lib.optionalAttrs (pkgs.stdenv.isDarwin) {
          default = pkgs.nix-installer;
        });

      hydraJobs = {
        vm-test = import ./nix/tests/vm-test {
          inherit forSystem;
          inherit (nix.hydraJobs) binaryTarball;
          inherit (nixpkgs) lib;
        };
        container-test = import ./nix/tests/container-test {
          inherit forSystem;
          inherit (nix.hydraJobs) binaryTarball;
        };
      };
    };
}
