{ pkgs, eclint, toolchain }:

let
  inherit (pkgs) writeShellApplication;
in
{

  # Format
  ci-check-rustfmt = (writeShellApplication {
    name = "ci-check-rustfmt";
    runtimeInputs = [ toolchain ];
    text = "cargo fmt --check";
  });

  # Spelling
  ci-check-spelling = (writeShellApplication {
    name = "ci-check-spelling";
    runtimeInputs = with pkgs; [ codespell ];
    text = ''
      codespell \
        --ignore-words-list ba,sur,crate,pullrequest,pullrequests,ser \
        --skip target \
        .
    '';
  });

  # NixFormatting
  ci-check-nixpkgs-fmt = (writeShellApplication {
    name = "ci-check-nixpkgs-fmt";
    runtimeInputs = with pkgs; [ git ];
    text = ''
      git ls-files '*.nix' | xargs | nixpkgs-fmt --check
    '';
  });

  # EditorConfig
  ci-check-editorconfig = (writeShellApplication {
    name = "ci-check-editorconfig";
    runtimeInputs = with pkgs; [ eclint ];
    text = ''
      eclint
    '';
  });
}
