import requests
import subprocess
import shutil 

response = requests.get('https://hydra.nixos.org/jobset/experimental-nix-installer/experimental-installer/evals', headers={'Accept': 'application/json'})

hydra_eval = response.json()['evals'][0]

for build_id in hydra_eval['builds']:
    response = requests.get(f"https://hydra.nixos.org/build/{build_id}", headers={'Accept': 'application/json'})
    build = response.json()
    installer_url = build['buildoutputs']['out']['path']
    system = build['system']
    subprocess.call(f"nix-store -r {installer_url}", shell=True)

    shutil.copy(f"{installer_url}/bin/nix-installer", f"nix-installer-{system}")