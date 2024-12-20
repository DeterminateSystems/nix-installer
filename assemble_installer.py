import requests
import subprocess
import shutil
import sys

response = requests.get('https://hydra.nixos.org/jobset/experimental-nix-installer/experimental-installer/evals', headers={'Accept': 'application/json'})

hydra_eval = response.json()['evals'][0]

installers = []

for build_id in hydra_eval['builds']:
    response = requests.get(f"https://hydra.nixos.org/build/{build_id}", headers={'Accept': 'application/json'})
    build = response.json()
    installer_url = build['buildoutputs']['out']['path']
    system = build['system']
    if build['finished'] == 1:
        try:
            subprocess.call(f"nix-store -r {installer_url}", shell=True)
        except:
            # retry once
            subprocess.call(f"nix-store -r {installer_url}", shell=True)
        installers.append((installer_url, system))
    else:
        sys.exit(0)

subprocess.run(["git", "fetch", "origin", "prerelease"], check=True)
subprocess.run(["git", "checkout", "-b", "prerelease", "origin/prerelease"], check=True)

for installer_url, system in installers:
    shutil.copy(f"{installer_url}/bin/nix-installer", f"nix-installer-{system}")
