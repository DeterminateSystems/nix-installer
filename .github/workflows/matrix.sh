#!/bin/sh

nix flake show --json --all-systems \
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
                | .[0][0:-1] # Take each attribute name and drop `type`
                | select(.[0:3] != ["hydraJobs", "vm-test", "all"]) # Skip the hydraJobs.vm-test.all jobs, which aggregate other jobs
                | select(.[0:3] != ["hydraJobs", "container-test", "all"]) # Skip the hydraJobs.container-test.all jobs, which aggregate other jobs
                | select(.[-1] != "all") # Skip attributes which are `all` jobs, presumably combining other jobs
                | select(.[-1] | endswith("-aggregate") != true) # Skip attributes which end in `-aggregate`, because those just depend on other jobs which build them
                | select(.[0] == "hydraJobs") # Select the hydraJobs which are not typically run in CI
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