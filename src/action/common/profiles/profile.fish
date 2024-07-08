if [ -f /nix/nix-installer ] && [ -x /nix/nix-installer ] && not set -q __ETC_PROFILE_NIX_SOURCED;
    eval "$(/nix/nix-installer export --format fish)"
end
