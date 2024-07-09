#!/bin/sh

#nix flake show --json --all-systems \
cat data.json \
    | nix run nixpkgs#gron -- -j \
    | nix run nixpkgs#jq -- -r \
        --slurp \
        --argjson map '
          {
            "aarch64-darwin": "macos-latest",
            "x86_64-darwin": "macos-latest",
            "x86_64-linux": "UbuntuLatest32Cores128G",
            "aarch64-linux": "UbuntuLatest32Cores128GArm",
            "i686-linux": "UbuntuLatest32Cores128G"
          }
        ' \
        '
            map(select(.[0][-1] == "type" and .[1] == "derivation")
                | .[0][0:-1]
                | select(.[-1] != "all")
                | {
                    attribute: . | join("."),
                    "nix-system": .[-2],
                    "runs-on": $map[.[-2]]
                  }
                | if ."runs-on" == null then
                   ("No GitHub Actions Runner system known for the Nix system `" + ."nix-system" + "` on attribute `" + .attribute + "`.\n") | halt_error(1)
                 else
                   .
                 end)
            | "matrix=" + tostring
        ' >> "$GITHUB_OUTPUT"