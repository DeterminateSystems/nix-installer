{
  description = "riff";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-22.05";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self
    , nixpkgs
    , fenix
    , naersk
    , ...
    } @ inputs:
    let
      nameValuePair = name: value: { inherit name value; };
      genAttrs = names: f: builtins.listToAttrs (map (n: nameValuePair n (f n)) names);
      allSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      forAllSystems = f: genAttrs allSystems (system: f rec {
        inherit system;
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
      });

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
    in
    {
      devShells = forAllSystems ({ system, pkgs, ... }:
        let
          toolchain = fenixToolchain system;
          ci = import ./nix/ci.nix { inherit pkgs; };
          eclint = import ./nix/eclint.nix { inherit pkgs; };

          spellcheck = pkgs.writeScriptBin "spellcheck" ''
            ${pkgs.codespell}/bin/codespell \
              --ignore-words-list crate,pullrequest,pullrequests,ser \
              --skip target \
              .
          '';
        in
        {
          default = pkgs.mkShell {
            name = "nix-install-shell";

            RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";

            nativeBuildInputs = with pkgs; [
              pkg-config
            ];
            buildInputs = with pkgs; [
              toolchain
              openssl
              rust-analyzer

              # CI dependencies
              jq
              codespell
              findutils # for xargs
              git
              nixpkgs-fmt
              eclint
            ]
            ++ ci
            ++ lib.optionals (pkgs.stdenv.isDarwin) (with pkgs; [ libiconv darwin.apple_sdk.frameworks.Security ]);
          };
        });

      packages = forAllSystems
        ({ system, pkgs, lib, ... }:
          let
            naerskLib = pkgs.callPackage naersk {
              cargo = fenixToolchain system;
              rustc = fenixToolchain system;
            };

            sharedAttrs = {
              pname = "harmonic";
              version = "0.0.0-unreleased";
              src = self;

              nativeBuildInputs = with pkgs; [
                pkg-config
              ];
              buildInputs = with pkgs; [

                openssl
              ] ++ lib.optionals (pkgs.stdenv.isDarwin) (with pkgs.darwin.apple_sdk.frameworks; [
                SystemConfiguration
              ]);

              doCheck = true;
              RUSTFLAGS = "--cfg tracing_unstable";

              override = { preBuild ? "", ... }: {
                preBuild = preBuild + ''
                  # logRun "cargo clippy --all-targets --all-features -- -D warnings"
                '';
              };
            };
          in
          rec {
            harmonic = naerskLib.buildPackage
              (sharedAttrs // { });
          } // lib.optionalAttrs (system == "x86_64-linux") rec {
            default = harmonicStatic;
            harmonicStatic = naerskLib.buildPackage
              (sharedAttrs // {
                CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
                OPENSSL_LIB_DIR = "${pkgs.pkgsStatic.openssl.out}/lib";
                OPENSSL_INCLUDE_DIR = "${pkgs.pkgsStatic.openssl.dev}";
              });
          } // lib.optionalAttrs (system == "aarch64-linux") rec {
            default = harmonicStatic;
            harmonicStatic = naerskLib.buildPackage
              (sharedAttrs // {
                CARGO_BUILD_TARGET = "aarch64-unknown-linux-musl";
                OPENSSL_LIB_DIR = "${pkgs.pkgsStatic.openssl.out}/lib";
                OPENSSL_INCLUDE_DIR = "${pkgs.pkgsStatic.openssl.dev}";
              });
          });

      defaultPackage = forAllSystems ({ system, ... }: self.packages.${system}.harmonic);
    };
}
