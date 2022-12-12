{
  description = "harmonic";

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
          eclint = import ./nix/eclint.nix { inherit pkgs; };
          check = import ./nix/check.nix { inherit pkgs eclint toolchain; };
        in
        {
          default = pkgs.mkShell {
            name = "nix-install-shell";

            RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";

            nativeBuildInputs = with pkgs; [ ];
            buildInputs = with pkgs; [
              toolchain
              rust-analyzer
              cargo-outdated
              nixpkgs-fmt
              check.check-rustfmt
              check.check-spelling
              check.check-nixpkgs-fmt
              check.check-editorconfig
            ]
            ++ lib.optionals (pkgs.stdenv.isDarwin) (with pkgs; [ libiconv ]);
          };
        });

      checks = forAllSystems ({ system, pkgs, ... }:
        let
          pkgs = import nixpkgs {
            inherit system;
          };
          toolchain = fenixToolchain system;
          eclint = import ./nix/eclint.nix { inherit pkgs; };
          check = import ./nix/check.nix { inherit pkgs eclint toolchain; };
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

              nativeBuildInputs = with pkgs; [ ];
              buildInputs = with pkgs; [ ] ++ lib.optionals (pkgs.stdenv.isDarwin) (with pkgs.darwin.apple_sdk.frameworks; [
                SystemConfiguration
              ]);

              doCheck = true;
              doDoc = true;
              doDocFail = true;
              RUSTFLAGS = "--cfg tracing_unstable --cfg tokio_unstable";
              cargoTestOptions = f: f ++ [ "--all" ];

              override = { preBuild ? "", ... }: {
                preBuild = preBuild + ''
                  # logRun "cargo clippy --all-targets --all-features -- -D warnings"
                '';
              };
              postInstall = ''
                cp nix-install.sh $out/bin/nix-install.sh
              '';
            };
          in
          rec {
            harmonic = naerskLib.buildPackage
              (sharedAttrs // { });
            default = self.packages.${system}.harmonic;
          } // lib.optionalAttrs (system == "x86_64-linux") rec {
            default = harmonicStatic;
            harmonicStatic = naerskLib.buildPackage
              (sharedAttrs // {
                CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
              });
          } // lib.optionalAttrs (system == "aarch64-linux") rec {
            default = harmonicStatic;
            harmonicStatic = naerskLib.buildPackage
              (sharedAttrs // {
                CARGO_BUILD_TARGET = "aarch64-unknown-linux-musl";
              });
          });
    };
}
