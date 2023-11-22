# shellcheck shell=sh
if [ -f /nix/nix-installer ] && [ -x /nix/nix-installer ] && [ -z "${__ETC_PROFILE_NIX_SOURCED:-}" ]; then
    if NIX_INSTALLER_EXPORT_DATA=$(/nix/nix-installer export --format space-newline-separated); then
        while read -r nix_installer_export_key nix_installer_export_value; do
            if [ -n "$nix_installer_export_key" ]; then
                export "$nix_installer_export_key=$nix_installer_export_value";
            fi
        done <<DATA_INPUT
    $NIX_INSTALLER_EXPORT_DATA
DATA_INPUT
        unset nix_installer_export_key nix_installer_export_value
    fi

    unset NIX_INSTALLER_EXPORT_DATA
fi
