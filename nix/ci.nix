{ pkgs }:

let
  inherit (pkgs) writeScriptBin;
in
[

  # Format
  (writeScriptBin "ci-check-rustfmt" "cargo fmt --check")

  # Test
  (writeScriptBin "ci-test-rust" "cargo test")

  # Spelling
  (writeScriptBin "ci-check-spelling" ''
    codespell \
      --ignore-words-list crate,pullrequest,pullrequests,ser \
      --skip target \
      .
  '')

  # NixFormatting
  (writeScriptBin "ci-check-nixpkgs-fmt" ''
    git ls-files '*.nix' | xargs | nixpkgs-fmt --check
  '')

  # RegistryFormatting
  (writeScriptBin "ci-check-registry-format" ''
    ./registry/format.sh && git diff --exit-code
  '')

  # EditorConfig
  (writeScriptBin "ci-check-editorconfig" ''
    eclint
  '')

  (writeScriptBin "ci-all" ''
    ci-check-rustfmt
    ci-test-rust
    ci-check-spelling
    ci-check-nixpkgs-fmt
    ci-check-registry-format
    ci-check-editorconfig
  '')
]
