# shellcheck shell=sh

if [ -f /nix/nix-installer ] && [ -x /nix/nix-installer ] && [ -z "${__ETC_PROFILE_NIX_SOURCED:-}" ]; then
    eval "$(/nix/nix-installer export --format sh)"
fi
