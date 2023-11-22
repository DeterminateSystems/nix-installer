if [ -f /nix/nix-installer ] && [ -x /nix/nix-installer ] && [ -z "${__ETC_PROFILE_NIX_SOURCED:-}" ]; then
    /nix/nix-installer export --format null-separated \
        | while read --null key
            read --export --null "$key"
        end
end
