#!/bin/sh

set -eux

version=$1

if ! echo "$version" | grep -q "^[[:digit:]]\+\.[[:digit:]]\+\.[[:digit:]]\+$"; then
  echo "argv[1] needs to be a version, in x.y.z format."
  exit 1
fi

git fetch
git checkout origin/main
git checkout -B "release-v$version"

sed -i '/^version = ".*"$/s/^.*/version = "'"$version"'"/' Cargo.toml
git add Cargo.toml

for fname in $(find ./tests/fixtures -name '*.json'); do
  cat "$fname" \
    | jq '.version = $version | .diagnostic_data.version = $version' --arg version "$version" \
    > "$fname.next"
  mv "$fname.next" "$fname"
  git add "$fname"
done

git commit -m "Update Cargo.toml and fixtures to v$version"

cargo update --aggressive
git add Cargo.lock
git commit -m "Update Cargo.lock prior to v$version"

nix flake update --commit-lock-file

cargo outdated --ignore-external-rel --aggressive

echo "Complete"
