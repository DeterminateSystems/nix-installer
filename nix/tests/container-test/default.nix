# Largely derived from https://github.com/NixOS/nix/blob/14f7dae3e4eb0c34192d0077383a7f2a2d630129/tests/installer/default.nix
{ forSystem, binaryTarball }:

let
  images = {

    "ubuntu-v22_04" = {
      tarball = import <nix/fetchurl.nix> {
        url = "https://launchpad.net/~cloud-images-release-managers/+livefs/ubuntu/jammy/ubuntu-oci/+build/408115/+files/livecd.ubuntu-oci.rootfs.tar.gz";
        hash = "sha256-BirwSM4c+ZV1upU0yV3qa+BW9AvpBUxvZuPTeI9mA8M=";
      };
      tag = "ubuntu:22.04";
      tester = ./ubuntu/22.04/Dockerfile;
      system = "x86_64-linux";
    };

  };

  makeTest = imageName:
    let image = images.${imageName}; in
    with (forSystem image.system ({ system, pkgs, lib, ... }: pkgs));
    testers.nixosTest
      {
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
          machine.succeed("podman import /image ${image.tag}")
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

