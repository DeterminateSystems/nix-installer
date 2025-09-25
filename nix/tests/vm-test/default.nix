# Largely derived from https://github.com/NixOS/nix/blob/14f7dae3e4eb0c34192d0077383a7f2a2d630129/tests/installer/default.nix
{ forSystem, binaryTarball, lib }:

let
  nix-installer-install = ''
    NIX_PATH=$(readlink -f nix.tar.xz)
    RUST_BACKTRACE="full" ./nix-installer install --nix-package-url "file://$NIX_PATH" --no-confirm --logger pretty --log-directive nix_installer=trace
  '';
  nix-installer-install-quiet = ''
    NIX_PATH=$(readlink -f nix.tar.xz)
    RUST_BACKTRACE="full" ./nix-installer install --nix-package-url "file://$NIX_PATH" --no-confirm
  '';
  nix-installer-install-determinate = ''
    NIX_PATH=$(readlink -f nix.tar.xz)
    RUST_BACKTRACE="full" ./nix-installer install --nix-package-url "file://$NIX_PATH" --no-confirm --logger pretty --log-directive nix_installer=trace --determinate
  '';
  cure-script-multi-user = ''
    tar xvf nix.tar.xz
    ./nix-*/install --no-channel-add --yes --daemon
  '';
  cure-script-single-user = ''
    tar xvf nix.tar.xz
    ./nix-*/install --no-channel-add --yes --no-daemon
  '';
  installCases = rec {
    install-default = {
      install = nix-installer-install;
      check = ''
        set -ex

        dir /nix
        dir /nix/store

        ls -lah /nix/var/nix/profiles/per-user
        ls -lah /nix/var/nix/daemon-socket

        if systemctl is-active nix-daemon.socket; then
          echo "nix-daemon.socket was active"
        else
          echo "nix-daemon.socket was not active, should be"
          exit 1
        fi
        if systemctl is-failed nix-daemon.socket; then
          echo "nix-daemon.socket is failed"
          sudo journalctl -eu nix-daemon.socket
          exit 1
        fi

        if !(sudo systemctl start nix-daemon.service); then
          echo "nix-daemon.service failed to start"
          sudo journalctl -eu nix-daemon.service
          exit 1
        fi

        if systemctl is-failed nix-daemon.service; then
          echo "nix-daemon.service is failed"
          sudo journalctl -eu nix-daemon.service
          exit 1
        fi

        if !(sudo systemctl stop nix-daemon.service); then
          echo "nix-daemon.service failed to stop"
          sudo journalctl -eu nix-daemon.service
          exit 1
        fi

        sudo -i nix store ping --store daemon
        nix store ping --store daemon

        sudo -i nix-env --version
        nix-env --version
        sudo -i nix --extra-experimental-features nix-command store ping
        nix --extra-experimental-features nix-command store ping

        out=$(nix-build --no-substitute -E 'derivation { name = "foo"; system = "x86_64-linux"; builder = "/bin/sh"; args = ["-c" "echo foobar > $out"]; }')
        [[ $(cat $out) = foobar ]]
      '';
      uninstall = ''
        /nix/nix-installer uninstall --no-confirm
      '';
      uninstallCheck = ''
        if which nix; then
          echo "nix existed on path after uninstall"
          exit 1
        fi

        for i in $(seq 1 32); do
          if id -u nixbld$i; then
            echo "User nixbld$i exists after uninstall"
            exit 1
          fi
        done
        if grep "^nixbld:" /etc/group; then
          echo "Group nixbld exists after uninstall"
          exit 1
        fi

        if sudo -i nix store ping --store daemon; then
          echo "Could run nix store ping after uninstall"
          exit 1
        fi

        if [ -d /nix/store ]; then
          echo "/nix/store exists after uninstall"
          exit 1
        fi
        if [ -d /nix/var ]; then
          echo "/nix/var exists after uninstall"
          exit 1
        fi

        if [ -d /etc/nix/nix.conf ]; then
          echo "/etc/nix/nix.conf exists after uninstall"
          exit 1
        fi

        if [ -f /etc/systemd/system/nix-daemon.socket ]; then
          echo "/etc/systemd/system/nix-daemon.socket exists after uninstall"
          exit 1
        fi

        if [ -f /etc/systemd/system/nix-daemon.service ]; then
          echo "/etc/systemd/system/nix-daemon.socket exists after uninstall"
          exit 1
        fi


        if systemctl status nix-daemon.socket > /dev/null; then
          echo "systemd unit nix-daemon.socket still exists after uninstall"
          exit 1
        fi

        if systemctl status nix-daemon.service > /dev/null; then
          echo "systemd unit nix-daemon.service still exists after uninstall"
          exit 1
        fi
      '';
    };
    install-no-start-daemon = {
      install = ''
        NIX_PATH=$(readlink -f nix.tar.xz)
        RUST_BACKTRACE="full" ./nix-installer install linux --nix-package-url "file://$NIX_PATH" --no-confirm --logger pretty --log-directive nix_installer=info --no-start-daemon
      '';
      check = ''
        set -ex

        if systemctl is-active nix-daemon.socket; then
          echo "nix-daemon.socket was running, should not be"
          exit 1
        fi
        if systemctl is-active nix-daemon.service; then
          echo "nix-daemon.service was running, should not be"
          exit 1
        fi
        sudo systemctl start nix-daemon.socket

        nix-env --version
        nix --extra-experimental-features nix-command store ping
        out=$(nix-build --no-substitute -E 'derivation { name = "foo"; system = "x86_64-linux"; builder = "/bin/sh"; args = ["-c" "echo foobar > $out"]; }')

        [[ $(cat $out) = foobar ]]
      '';
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    install-daemonless = {
      install = ''
        NIX_PATH=$(readlink -f nix.tar.xz)
        RUST_BACKTRACE="full" ./nix-installer install linux --nix-package-url "file://$NIX_PATH" --no-confirm --logger pretty --log-directive nix_installer=info --init none
      '';
      check = ''
        set -ex
        sudo -i nix-env --version
        sudo -i nix --extra-experimental-features nix-command store ping

        echo 'derivation { name = "foo"; system = "x86_64-linux"; builder = "/bin/sh"; args = ["-c" "echo foobar > $out"]; }' | sudo tee -a /drv
        out=$(sudo -i nix-build --no-substitute /drv)

        [[ $(cat $out) = foobar ]]
      '';
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    install-bind-mounted-nix = {
      preinstall = ''
        sudo mkdir -p /nix
        sudo mkdir -p /bind-mount-for-nix
        sudo mount --bind /bind-mount-for-nix /nix
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    install-invalid-custom-conf = {
      preinstall = ''
        sudo mkdir -p /etc/nix
        sudo touch /etc/nix/nix.custom.conf
        sudo chmod 777 /etc/nix/nix.custom.conf
        echo "foobar" > /etc/nix/nix.custom.conf
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check + ''
        grep --quiet "^# foobar" /etc/nix/nix.custom.conf
      '';
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    install-determinate = {
      install = nix-installer-install-determinate;
      check = ''
        if ! systemctl is-active determinate-nixd.socket; then
          echo "determinate-nixd.socket is not active"
          sudo journalctl -eu determinate-nixd.socket
          exit 1
        fi

        if ! determinate-nixd status; then
          echo "determinate-nixd is not working"
          sudo journalctl -eu determinate-nixd.service
          exit 1
        fi
      '' + installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
  };
  cureSelfCases = {
    cure-self-linux-working = {
      preinstall = ''
        ${nix-installer-install-quiet}
        sudo mv /nix/receipt.json /nix/old-receipt.json
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-self-linux-broken-no-nix-path = {
      preinstall = ''
        NIX_PATH=$(readlink -f nix.tar.xz)
        RUST_BACKTRACE="full" ./nix-installer install --nix-package-url "file://$NIX_PATH" --no-confirm
        sudo mv /nix/receipt.json /nix/old-receipt.json
        sudo rm -rf /nix/
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-self-linux-broken-missing-users = {
      preinstall = ''
        ${nix-installer-install-quiet}
        sudo mv /nix/receipt.json /nix/old-receipt.json
        sudo userdel nixbld1
        sudo userdel nixbld3
        sudo userdel nixbld16
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-self-linux-broken-missing-users-and-group = {
      preinstall = ''
        NIX_PATH=$(readlink -f nix.tar.xz)
        RUST_BACKTRACE="full" ./nix-installer install --nix-package-url "file://$NIX_PATH" --no-confirm
        sudo mv /nix/receipt.json /nix/old-receipt.json
        for i in {1..32}; do
          sudo userdel "nixbld''${i}"
        done
        sudo groupdel nixbld
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-self-linux-broken-daemon-disabled = {
      preinstall = ''
        ${nix-installer-install-quiet}
        sudo mv /nix/receipt.json /nix/old-receipt.json
        sudo systemctl disable --now nix-daemon.socket
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-self-multi-broken-daemon-stopped = {
      preinstall = ''
        ${nix-installer-install-quiet}
        sudo mv /nix/receipt.json /nix/old-receipt.json
        sudo systemctl stop nix-daemon.socket
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-self-linux-broken-no-etc-nix = {
      preinstall = ''
        ${nix-installer-install-quiet}
        sudo mv /nix/receipt.json /nix/old-receipt.json
        sudo rm -rf /etc/nix
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-self-linux-broken-unmodified-bashrc = {
      preinstall = ''
        ${nix-installer-install-quiet}
        sudo mv /nix/receipt.json /nix/old-receipt.json
        sudo sed -i '/# Nix/,/# End Nix/d' /etc/bash.bashrc
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
  };
  cureScriptCases = {
    cure-script-multi-self-broken-no-nix-path = {
      preinstall = ''
        ${cure-script-multi-user}
        sudo rm -rf /nix/
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-script-multi-broken-missing-users = {
      preinstall = ''
        ${cure-script-multi-user}
        sudo userdel nixbld1
        sudo userdel nixbld3
        sudo userdel nixbld16
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-script-multi-broken-daemon-disabled = {
      preinstall = ''
        ${cure-script-multi-user}
        sudo systemctl disable --now nix-daemon.socket
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-script-multi-broken-daemon-stopped = {
      preinstall = ''
        ${cure-script-multi-user}
        sudo systemctl stop nix-daemon.socket
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-script-multi-broken-no-etc-nix = {
      preinstall = ''
        ${cure-script-multi-user}
        sudo rm -rf /etc/nix
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-script-multi-broken-unmodified-bashrc = {
      preinstall = ''
        ${cure-script-multi-user}
        sudo sed -i '/# Nix/,/# End Nix/d' /etc/bash.bashrc
      '';
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    cure-script-multi-working = {
      preinstall = cure-script-multi-user;
      install = installCases.install-default.install;
      check = installCases.install-default.check;
      uninstall = installCases.install-default.uninstall;
      uninstallCheck = installCases.install-default.uninstallCheck;
    };
    # cure-script-single-working = {
    #   preinstall = cure-script-single-user;
    #   install = installCases.install-default.install;
    #   check = installCases.install-default.check;
    # };
  };
  # Cases to test uninstalling is complete even in the face of errors.
  uninstallCases =
    let
      uninstallFailExpected = ''
        if /nix/nix-installer uninstall --no-confirm; then
          echo "/nix/nix-installer uninstall exited with 0 during a uninstall failure test"
          exit 1
        else
          exit 0
        fi
      '';
    in
    {
      uninstall-users-and-groups-missing = {
        install = installCases.install-default.install;
        check = installCases.install-default.check;
        preuninstall = ''
          for i in $(seq 1 32); do
            sudo userdel nixbld$i
          done
          sudo groupdel nixbld
        '';
        uninstall = uninstallFailExpected;
        uninstallCheck = installCases.install-default.uninstallCheck;
      };
      uninstall-nix-conf-gone = {
        install = installCases.install-default.install;
        check = installCases.install-default.check;
        preuninstall = ''
          sudo rm -rf /etc/nix
        '';
        uninstall = uninstallFailExpected;
        uninstallCheck = installCases.install-default.uninstallCheck;
      };
    };

  disableSELinux = "sudo setenforce 0";

  images = {

    # End of standard support https://wiki.ubuntu.com/Releases
    "ubuntu-v20_04" = {
      image = import <nix/fetchurl.nix> {
        url = "https://app.vagrantup.com/generic/boxes/ubuntu2004/versions/4.3.12/providers/libvirt.box";
        hash = "sha256-lo6fkz6N/Q9mdD+RWoUssak9TVod0F7QSgZvxnMj9IQ=";
      };
      rootDisk = "box.img";
      system = "x86_64-linux";
    };

    "ubuntu-v22_04" = {
      image = import <nix/fetchurl.nix> {
        url = "https://app.vagrantup.com/generic/boxes/ubuntu2204/versions/4.3.12/providers/libvirt.box";
        hash = "sha256-xiUt25lRPIpM/HKepJbS15Pp9+PraPR1BLT6lVZko5k=";
      };
      rootDisk = "box.img";
      system = "x86_64-linux";
    };

    "fedora-v38" = {
      image = import <nix/fetchurl.nix> {
        url = "https://app.vagrantup.com/generic/boxes/fedora38/versions/4.3.12/providers/libvirt.box";
        hash = "sha256-nz+sW771/P6yeUJmpfUYSTYxLFgq0mkfu8h/mflhPV8=";
      };
      rootDisk = "box.img";
      system = "x86_64-linux";
      upstreamScriptsWork = false; # SELinux!
    };

    "fedora-v39" = {
      image = import <nix/fetchurl.nix> {
        url = "https://app.vagrantup.com/generic/boxes/fedora39/versions/4.3.12/providers/libvirt.box";
        hash = "sha256-VJbWmcy3XiEm7cUAXtod8VlFwsIwnVYlZ/LYTuoj9WI=";
      };
      rootDisk = "box.img";
      system = "x86_64-linux";
      upstreamScriptsWork = false; # SELinux!
    };

    "rhel-v8" = {
      image = import <nix/fetchurl.nix> {
        url = "https://app.vagrantup.com/generic/boxes/rhel8/versions/4.3.12/providers/libvirt.box";
        hash = "sha256-1BTKt196Qb381zkyzPTOzzbnMGbIloebEpyycLu2myY=";
      };
      rootDisk = "box.img";
      system = "x86_64-linux";
      upstreamScriptsWork = false; # SELinux!
    };

    "rhel-v9" = {
      image = import <nix/fetchurl.nix> {
        url = "https://app.vagrantup.com/generic/boxes/rhel9/versions/4.3.12/providers/libvirt.box";
        hash = "sha256-Gp3VSqSOnyysHFqQDoeBjYrshEHfqQw3XPzS3VxZaPA=";
      };
      rootDisk = "box.img";
      system = "x86_64-linux";
      upstreamScriptsWork = false; # SELinux!
      extraQemuOpts = "-cpu Westmere-v2";
    };

  };

  makeTest = imageName: testName: test:
    let image = images.${imageName}; in
    with (forSystem image.system ({ system, pkgs, ... }: pkgs));
    runCommand
      "installer-test-${imageName}-${testName}"
      {
        buildInputs = [ qemu_kvm openssh ];
        image = image.image;
        postBoot = image.postBoot or "";
        preinstallScript = test.preinstall or "echo \"Not Applicable\"";
        installScript = test.install;
        checkScript = test.check;
        uninstallScript = test.uninstall;
        preuninstallScript = test.preuninstall or "echo \"Not Applicable\"";
        uninstallCheckScript = test.uninstallCheck;
        installer = nix-installer-static;
        binaryTarball = binaryTarball.${system};
      }
      ''
        shopt -s nullglob

        echo "Unpacking Vagrant box $image..."
        tar xvf $image

        image_type=$(qemu-img info ${image.rootDisk} | sed 's/file format: \(.*\)/\1/; t; d')

        qemu-img create -b ./${image.rootDisk} -F "$image_type" -f qcow2 ./disk.qcow2

        extra_qemu_opts="${image.extraQemuOpts or ""}"

        # Add the config disk, required by the Ubuntu images.
        config_drive=$(echo *configdrive.vmdk || true)
        if [[ -n $config_drive ]]; then
          extra_qemu_opts+=" -drive id=disk2,file=$config_drive,if=virtio"
        fi

        echo "Starting qemu..."
        qemu-kvm -m 4096 -nographic \
          -drive id=disk1,file=./disk.qcow2,if=virtio \
          -netdev user,id=net0,restrict=yes,hostfwd=tcp::20022-:22 -device virtio-net-pci,netdev=net0 \
          $extra_qemu_opts &
        qemu_pid=$!
        trap "kill $qemu_pid" EXIT

        if ! [ -e ./vagrant_insecure_key ]; then
          cp ${./vagrant_insecure_key} vagrant_insecure_key
        fi

        chmod 0400 ./vagrant_insecure_key

        ssh_opts="-o StrictHostKeyChecking=no -o HostKeyAlgorithms=+ssh-rsa -o PubkeyAcceptedKeyTypes=+ssh-rsa -i ./vagrant_insecure_key"
        ssh="ssh -p 20022 -q $ssh_opts vagrant@localhost"

        echo "Waiting for SSH..."
        for ((i = 0; i < 120; i++)); do
          echo "[ssh] Trying to connect..."
          if $ssh -- true; then
            echo "[ssh] Connected!"
            break
          fi
          if ! kill -0 $qemu_pid; then
            echo "qemu died unexpectedly"
            exit 1
          fi
          sleep 1
        done

        if [[ -n $postBoot ]]; then
          echo "Running post-boot commands..."
          $ssh "set -ex; $postBoot"
        fi

        echo "Copying installer..."
        scp -P 20022 $ssh_opts $installer/bin/nix-installer vagrant@localhost:nix-installer

        echo "Copying nix tarball..."
        scp -P 20022 $ssh_opts $binaryTarball/nix-*.tar.xz vagrant@localhost:nix.tar.xz

        echo "Running preinstall..."
        $ssh "set -eux; $preinstallScript"

        echo "Running installer..."
        $ssh "set -eux; $installScript"

        echo "Checking Nix installation..."
        $ssh "set -eux; $checkScript"

        echo "Running preuninstall..."
        $ssh "set -eux; $preuninstallScript"

        echo "Running Nix uninstallation..."
        $ssh "set -eux; $uninstallScript"

        echo "Checking Nix uninstallation..."
        $ssh "set -eux; $uninstallCheckScript"

        echo "Done!"
        touch $out
      '';

  makeTests = name: tests: builtins.mapAttrs
    (imageName: image:
      let
        doTests = builtins.removeAttrs tests (image.skip or [ ]);
      in
      rec {
        ${image.system} = (builtins.mapAttrs
          (testName: test:
            makeTest imageName testName test
          )
          doTests) // {
          "${name}" = (with (forSystem "x86_64-linux" ({ system, pkgs, ... }: pkgs)); pkgs.releaseTools.aggregate {
            name = name;
            constituents = (
              pkgs.lib.mapAttrsToList
                (testName: test:
                  makeTest imageName testName test
                )
                doTests
            );
          });
        };
      }
    )
    images;

  allCases = lib.recursiveUpdate (lib.recursiveUpdate installCases (lib.recursiveUpdate cureSelfCases cureScriptCases)) uninstallCases;

  install-tests = makeTests "install" installCases;

  cure-self-tests = makeTests "cure-self" cureSelfCases;

  cure-script-tests = makeTests "cure-script" cureScriptCases;

  uninstall-tests = makeTests "uninstall" uninstallCases;

  all-tests = builtins.mapAttrs
    (imageName: image: {
      "x86_64-linux".all = (with (forSystem "x86_64-linux" ({ system, pkgs, ... }: pkgs)); pkgs.releaseTools.aggregate {
        name = "all";
        constituents = [
          install-tests."${imageName}"."x86_64-linux".install
          cure-self-tests."${imageName}"."x86_64-linux".cure-self
          uninstall-tests."${imageName}"."x86_64-linux".uninstall
        ] ++ (lib.optional (image.upstreamScriptsWork or false) cure-script-tests."${imageName}"."x86_64-linux".cure-script);
      });
    })
    images;

  joined-tests = lib.recursiveUpdate (lib.recursiveUpdate (lib.recursiveUpdate install-tests (lib.recursiveUpdate cure-self-tests cure-script-tests)) uninstall-tests) all-tests;

in
lib.recursiveUpdate joined-tests {
  all."x86_64-linux" = (with (forSystem "x86_64-linux" ({ system, pkgs, ... }: pkgs)); pkgs.lib.mapAttrs (caseName: case:
    pkgs.releaseTools.aggregate {
      name = caseName;
      constituents = pkgs.lib.mapAttrsToList (name: value: value."x86_64-linux"."${caseName}" or "") joined-tests;
    }
  )) (allCases // { "cure-self" = { }; "cure-script" = { }; "install" = { }; "uninstall" = { }; "all" = { }; });
}
