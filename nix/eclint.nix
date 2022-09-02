{ pkgs }:

pkgs.buildGoModule
rec {
  pname = "eclint";
  version = "0.3.3";

  src = pkgs.fetchFromGitHub {
    owner = "greut";
    repo = pname;
    rev = "v${version}";
    sha256 = "sha256-9i2oAqFXflWGeBumE/5njaafBRhuRQSbA/ggUS72fwk=";
  };

  vendorSha256 = "sha256-XAyHy7UAb2LgwhsxaJgj0Qy6ukw9szeRC9JkRb+zc0Y=";
}
