#!/bin/sh

set -eux


main() {
version=$1

if ! echo "$version" | grep -q "^[[:digit:]]\+\.[[:digit:]]\+\.[[:digit:]]\+$"; then
  echo "argv[1] needs to be a version, in x.y.z format."
  exit 1
fi

git fetch
git checkout origin/main
git checkout -B "release-v$version"

nix flake update --commit-lock-file

cargo update --aggressive
git add Cargo.lock
git commit -m "Update Cargo.lock dependencies"

toml set ./Cargo.toml package.version "$version" > Cargo.toml.next
mv Cargo.toml.next Cargo.toml
git add Cargo.toml

cargo fetch
git add Cargo.lock

for fname in $(find ./tests/fixtures -name '*.json'); do
  cat "$fname" \
    | jq '.version = $version' --arg version "$version" \
    > "$fname.next"
  mv "$fname.next" "$fname"
  git add "$fname"
done

git commit -m "Release v$version"

cargo outdated --ignore-external-rel --aggressive

echo "Complete"
}

main "$@"
