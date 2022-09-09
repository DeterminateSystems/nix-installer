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
      --ignore-words-list ba,sur,crate,pullrequest,pullrequests,ser \
      --skip target \
      .
  '')

  # NixFormatting
  (writeScriptBin "ci-check-nixpkgs-fmt" ''
    git ls-files '*.nix' | xargs | nixpkgs-fmt --check
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
    ci-check-editorconfig
  '')
]
