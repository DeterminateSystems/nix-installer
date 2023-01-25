# Largely derived from https://github.com/NixOS/nix/blob/14f7dae3e4eb0c34192d0077383a7f2a2d630129/tests/installer/default.nix
{ forSystem, binaryTarball }:

let
  images = {

    # Found via https://hub.docker.com/_/ubuntu/ under "How is the rootfs build?"
    # Jammy
    "ubuntu-v22_04" = {
      tarball = import <nix/fetchurl.nix> {
        url = "https://launchpad.net/~cloud-images-release-managers/+livefs/ubuntu/jammy/ubuntu-oci/+build/408115/+files/livecd.ubuntu-oci.rootfs.tar.gz";
        hash = "sha256-BirwSM4c+ZV1upU0yV3qa+BW9AvpBUxvZuPTeI9mA8M=";
      };
      tester = ./default/Dockerfile;
      system = "x86_64-linux";
    };

    # focal
    "ubuntu-v20_04" = {
      tarball = import <nix/fetchurl.nix> {
        url = "https://launchpad.net/~cloud-images-release-managers/+livefs/ubuntu/focal/ubuntu-oci/+build/408120/+files/livecd.ubuntu-oci.rootfs.tar.gz";
        hash = "sha256-iTJR+DeC5lT4PMqT/xFAFwmlC/qvslDFccDrVFLt/a8=";
      };
      tester = ./default/Dockerfile;
      system = "x86_64-linux";
    };

    # bionic
    "ubuntu-v18_04" = {
      tarball = import <nix/fetchurl.nix> {
        url = "https://launchpad.net/~cloud-images-release-managers/+livefs/ubuntu/bionic/ubuntu-oci/+build/408103/+files/livecd.ubuntu-oci.rootfs.tar.gz";
        hash = "sha256-gi48yl5laoKLoVCDIORsseOM6DI58FNpAjSVe7OOs7I=";
      };
      tester = ./default/Dockerfile;
      system = "x86_64-linux";
    };

  };

  makeTest = imageName:
    let image = images.${imageName}; in
    with (forSystem image.system ({ system, pkgs, lib, ... }: pkgs));
    testers.nixosTest
      {
        name = "container-test-${imageName}";
        nodes = {
          machine =
            { config, pkgs, ... }: {
              virtualisation.podman.enable = true;
            };
        };
        # installer = nix-installer-static;
        # binaryTarball = binaryTarball.${system};
        testScript = ''
          machine.start()
          machine.copy_from_host("${image.tarball}", "/image")
          machine.succeed("mkdir -p /test")
          machine.copy_from_host("${image.tester}", "/test/Dockerfile")
          machine.copy_from_host("${nix-installer-static}", "/test/nix-installer")
          machine.copy_from_host("${binaryTarball.${system}}", "/test/binary-tarball")
          machine.succeed("podman import /image default")
          machine.succeed("podman build -t test /test")
        '';
      };

  container-tests = builtins.mapAttrs
    (imageName: image: {
      ${image.system} = makeTest imageName;
    })
    images;

in
container-tests // rec {
  all."x86_64-linux" = (with (forSystem "x86_64-linux" ({ system, pkgs, ... }: pkgs)); pkgs.releaseTools.aggregate {
    name = "all";
    constituents = pkgs.lib.mapAttrsToList (name: value: value."x86_64-linux") container-tests;
  });
}

