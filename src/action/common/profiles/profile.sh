# shellcheck shell=sh
if [ -f /nix/nix-installer ] && [ -x /nix/nix-installer ] && [ -z "${__ETC_PROFILE_NIX_SOURCED:-}" ]; then
    NIX_INSTALLER_EXPORT_DATA=$(/nix/nix-installer export --verbose --verbose --format space-newline-separated);
    while read -r nix_installer_export_key nix_installer_export_value; do
        export "$nix_installer_export_key=$nix_installer_export_value";
    done <<DATA_INPUT
$NIX_INSTALLER_EXPORT_DATA
DATA_INPUT

    unset NIX_INSTALLER_EXPORT_DATA nix_installer_export_key nix_installer_export_value
fi
