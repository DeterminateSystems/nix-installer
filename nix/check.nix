{ pkgs }:

let
  inherit (pkgs) writeShellApplication;
in
{

  # Format
  check-rustfmt = (
    writeShellApplication {
      name = "check-rustfmt";
      runtimeInputs = with pkgs; [
        cargo
        rustfmt
      ];
      text = "cargo fmt --check";
    }
  );

  # Spelling
  check-spelling = (
    writeShellApplication {
      name = "check-spelling";
      runtimeInputs = with pkgs; [
        git
        codespell
      ];
      text = ''
        codespell \
          --ignore-words-list="ba,sur,crate,pullrequest,pullrequests,ser,distroname" \
          --skip="./target,.git,./src/action/linux/selinux,*.lock" \
          .
      '';
    }
  );

  # Check Nix formatting
  check-nixfmt = (
    writeShellApplication {
      name = "check-nixfmt";
      runtimeInputs = with pkgs; [
        git
        nixfmt
      ];
      text = ''
        git ls-files '*.nix' | xargs nixfmt --check
      '';
    }
  );

  # Format Nix
  format-nix = (
    writeShellApplication {
      name = "format-nix";
      runtimeInputs = with pkgs; [
        git
        nixfmt
      ];
      text = ''
        git ls-files '*.nix' | xargs nix fmt
      '';
    }
  );

  # EditorConfig
  check-editorconfig = (
    writeShellApplication {
      name = "check-editorconfig";
      runtimeInputs = with pkgs; [ editorconfig-checker ];
      text = ''
        editorconfig-checker
      '';
    }
  );

  # Semver
  check-semver = (
    writeShellApplication {
      name = "check-semver";
      runtimeInputs = with pkgs; [ cargo-semver-checks ];
      text = ''
        cargo-semver-checks semver-checks check-release
      '';
    }
  );
  # Clippy
  check-clippy = (
    writeShellApplication {
      name = "check-clippy";
      runtimeInputs = with pkgs; [
        cargo
        clippy
        rustc
      ];
      text = ''
        cargo clippy
      '';
    }
  );

}
