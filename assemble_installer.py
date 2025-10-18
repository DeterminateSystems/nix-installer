#!/usr/bin/env python3
"""Assemble and release nix-installer binaries from Hydra builds."""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
import tomllib
import urllib.request
from string import Template
from typing import Any


def get_hydra_evals() -> list[dict[str, Any]]:
    """Fetch evaluations from Hydra jobset."""
    url = "https://hydra.nixos.org/jobset/experimental-nix-installer/experimental-installer/evals"
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read().decode("utf-8"))
    return data["evals"]


def find_eval(evals: list[dict[str, Any]], eval_id: str | None) -> dict[str, Any]:
    """Find the specified eval or return the latest one."""
    if eval_id is not None and eval_id != "":
        eval_id_int = int(eval_id)
        return next(eval for eval in evals if eval["id"] == eval_id_int)
    else:
        # Use latest eval and verify it matches current HEAD
        hydra_eval = evals[0]
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            stdout=subprocess.PIPE,
            check=True,
            text=True,
        )
        rev = result.stdout.strip()

        if rev not in hydra_eval["flake"]:
            raise RuntimeError(
                f"Expected flake with rev {rev} but found flake {hydra_eval['flake']}"
            )

        return hydra_eval


def get_build_info(build_id: int) -> dict[str, Any]:
    """Fetch build information from Hydra."""
    url = f"https://hydra.nixos.org/build/{build_id}"
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(req) as response:
        return json.loads(response.read().decode("utf-8"))


def download_installer(installer_url: str) -> bool:
    """Download installer using nix-store, with retry logic."""
    try:
        subprocess.run(
            f"nix-store -r {installer_url}",
            shell=True,
            check=True,
        )
        return True
    except subprocess.CalledProcessError:
        # Retry once
        try:
            subprocess.run(
                f"nix-store -r {installer_url}",
                shell=True,
                check=True,
            )
            return True
        except subprocess.CalledProcessError:
            return False


def get_version() -> str:
    """Extract version from Cargo.toml."""
    with open("Cargo.toml", "rb") as f:
        cargo_toml = tomllib.load(f)
    return cargo_toml["package"]["version"]


def create_release(version: str, release_files: list[str]) -> None:
    """Create a draft GitHub release with the given files."""
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


def main() -> None:
    """Main entry point for the installer assembly script."""
    parser = argparse.ArgumentParser(
        description="Assemble and release nix-installer binaries from Hydra builds"
    )
    parser.add_argument(
        "eval_id",
        nargs="?",
        default=None,
        help="Hydra evaluation ID to use (defaults to latest matching HEAD)",
    )
    args = parser.parse_args()

    # Fetch and select the evaluation
    evals = get_hydra_evals()
    hydra_eval = find_eval(evals, args.eval_id)

    # Process all builds in the evaluation
    installers: list[tuple[str, str]] = []
    for build_id in hydra_eval["builds"]:
        build = get_build_info(build_id)
        installer_url = build["buildoutputs"]["out"]["path"]
        system = build["system"]

        if build["finished"] == 1:
            if download_installer(installer_url):
                installers.append((installer_url, system))
        else:
            print(
                f"Build {build_id} not finished. "
                f"Check status at https://hydra.nixos.org/eval/{hydra_eval['id']}#tabs-unfinished"
            )
            sys.exit(0)

    # Get version from Cargo.toml
    version = get_version()

    # Create release with all installer binaries
    with tempfile.TemporaryDirectory() as tmpdirname:
        release_files: list[str] = []

        # Copy installer binaries
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
        updated_content = template.safe_substitute(
            assemble_installer_templated_version=version
        )

        # Write the modified content to the output file
        substituted_file = f"{tmpdirname}/nix-installer.sh"
        with open(substituted_file, "w", encoding="utf-8") as output_file:
            output_file.write(updated_content)
        release_files.append(substituted_file)

        # Create the GitHub release
        create_release(version, release_files)


if __name__ == "__main__":
    main()
