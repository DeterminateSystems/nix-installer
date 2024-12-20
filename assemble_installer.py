import os
import requests
import subprocess
import shutil
import sys
import tempfile
import tomllib

from string import Template

# A Hydra eval id must be passed as the argument to this script or an empty
# string to use latest eval on Hydra
# TODO: using an empty string is not the cleanest
# TODO: print script usage
# TODO: argparse or something
if len(sys.argv) < 2:
    eval_id = None
else:
    eval_id = sys.argv[1]

response = requests.get('https://hydra.nixos.org/jobset/experimental-nix-installer/experimental-installer/evals', headers={'Accept': 'application/json'})
evals = response.json()['evals']

if eval_id is not None and eval_id != "":
    eval_id_int = int(eval_id)
    ids = [eval['id'] for eval in evals]
    hydra_eval = next( eval for eval in evals if eval['id'] == eval_id_int )
else:
    hydra_eval = evals[0]

    rev = subprocess.run(
        ["git", "rev-parse", "HEAD"], stdout=subprocess.PIPE, check=True, text=True
    ).stdout.strip()

    if not rev in hydra_eval["flake"]:
        raise RuntimeError(
            f"Expected flake with rev {rev} but found flake {hydra_eval['flake']}"
        )

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
        print(
            f"Build {build_id} not finished. Check status at https://hydra.nixos.org/eval/{hydra_eval['id']}#tabs-unfinished"
        )
        sys.exit(0)

with open("Cargo.toml", "rb") as f:
    cargo_toml = tomllib.load(f)
version = cargo_toml["package"]["version"]

with tempfile.TemporaryDirectory() as tmpdirname:
    release_files = []
    for installer_url, system in installers:
        installer_file = f"{tmpdirname}/nix-installer-{system}"
        release_files.append(installer_file)
        print(f"Copying {installer_url} to {installer_file}")
        shutil.copy(f"{installer_url}/bin/nix-installer", installer_file)

    # Substitute version in nix-installer.sh
    original_file = "nix-installer.sh"

    with open(original_file, "r") as nix_installer_sh:
        nix_installer_sh_contents = nix_installer_sh.read()

    template = Template(nix_installer_sh_contents)
    updated_content = template.safe_substitute(assemble_installer_templated_version=version)

    # Write the modified content to the output file
    substituted_file=f"{tmpdirname}/nix-installer.sh"
    with open(substituted_file, "w", encoding="utf-8") as output_file:
        output_file.write(updated_content)
    release_files.append(substituted_file)

    subprocess.run(
        [
            "gh",
            "release",
            "create",
            "--notes",
            f"Release experimental nix installer v{version}",
            "--title",
            f"v{version}",
            "--draft",
            version,
            *release_files,
        ],
        check=True,
    )
