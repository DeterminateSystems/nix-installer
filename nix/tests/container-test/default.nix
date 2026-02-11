# Largely derived from https://github.com/NixOS/nix/blob/14f7dae3e4eb0c34192d0077383a7f2a2d630129/tests/installer/default.nix
{ forSystem, binaryTarball }:

let
  images = {

    # Found via https://hub.docker.com/_/ubuntu/ under "How is the rootfs build?"
    # Jammy
    "ubuntu-v22_04" = {
      tarball = builtins.fetchurl {
        url = "http://cdimage.ubuntu.com/ubuntu-base/releases/22.04/release/ubuntu-base-22.04-base-amd64.tar.gz";
        sha256 = "01sbpjb32x1z1yr9q78zrk0a6kfw5c4fxw1jqmm23g8ixryffvyz";
      };
      tester = ./default/Dockerfile;
      system = "x86_64-linux";
    };

    # focal
    "ubuntu-v20_04" = {
      tarball = builtins.fetchurl {
        url = "http://cdimage.ubuntu.com/ubuntu-base/releases/20.04/release/ubuntu-base-20.04.1-base-amd64.tar.gz";
        sha256 = "0ryn38csmx41a415g9b3wk30csaxxlkgkdij9v4754pk877wpxlp";
      };
      tester = ./default/Dockerfile;
      system = "x86_64-linux";
    };

    # bionic
    "ubuntu-v18_04" = {
      tarball = builtins.fetchurl {
        url = "http://cdimage.ubuntu.com/ubuntu-base/releases/18.04/release/ubuntu-base-18.04.5-base-amd64.tar.gz";
        sha256 = "1sh73pqwgyzkyssv3ngpxa2ynnkbdvjpxdw1v9ql4ghjpd3hpwlg";
      };
      tester = ./default/Dockerfile;
      system = "x86_64-linux";
    };
  };

  makeTest =
    containerTool: imageName:
    let
      image = images.${imageName};
    in
    with (forSystem image.system (
      {
        system,
        pkgs,
        lib,
        ...
      }:
      pkgs
    ));
    testers.nixosTest {
      name = "container-test-${imageName}";
      nodes = {
        machine =
          { config, pkgs, ... }:
          {
            virtualisation.${containerTool}.enable = true;
            virtualisation.diskSize = 4 * 1024;
          };
      };
      testScript = ''
        machine.start()
        machine.copy_from_host("${image.tarball}", "/image")
        machine.succeed("mkdir -p /test")
        machine.copy_from_host("${image.tester}", "/test/Dockerfile")
        machine.copy_from_host("${nix-installer-static}", "/test/nix-installer")
        machine.copy_from_host("${binaryTarball.${system}}", "/test/binary-tarball")
        machine.succeed("${containerTool} import /image default")
        machine.succeed("${containerTool} build -t test /test")
      '';
    };

  container-tests = builtins.mapAttrs (
    imageName: image:
    (with (forSystem "x86_64-linux" ({ system, pkgs, ... }: pkgs)); {
      ${image.system} = rec {
        docker = makeTest "docker" imageName;
        podman = makeTest "podman" imageName;
        all = pkgs.releaseTools.aggregate {
          name = "all";
          constituents = [
            docker
            podman
          ];
        };
      };
    })
  ) images;

in
container-tests
// {
  all."x86_64-linux" = rec {
    all = (
      with (forSystem "x86_64-linux" ({ system, pkgs, ... }: pkgs));
      pkgs.releaseTools.aggregate {
        name = "all";
        constituents = [
          docker
          podman
        ];
      }
    );
    docker = (
      with (forSystem "x86_64-linux" ({ system, pkgs, ... }: pkgs));
      pkgs.releaseTools.aggregate {
        name = "all";
        constituents = pkgs.lib.mapAttrsToList (name: value: value."x86_64-linux".docker) container-tests;
      }
    );
    podman = (
      with (forSystem "x86_64-linux" ({ system, pkgs, ... }: pkgs));
      pkgs.releaseTools.aggregate {
        name = "all";
        constituents = pkgs.lib.mapAttrsToList (name: value: value."x86_64-linux".podman) container-tests;
      }
    );
  };
}
