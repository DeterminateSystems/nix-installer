#! /usr/bin/env nix-shell
#! nix-shell -i bash -p libselinux -p semodule-utils -p checkpolicy

checkmodule -M -m -c 5 -o nix.mod nix.te
semodule_package -o nix.pp -m nix.mod -f nix.fc