set -eu

DEST="$1"
GIT_ISH="$2"
DEST_INSTALL_URL="$3"

is_tag() {
  if [[ "$GITHUB_REF_TYPE" == "tag" ]]; then
    return 0
  else
    return 1
  fi
}

# If the revision directory has already been created in S3 somehow, we don't want to reupload
if aws s3 ls "$AWS_BUCKET"/"$GIT_ISH"/; then
  # Only exit if it's not a tag (since we're tagging a commit previously pushed to main)
  if ! is_tag; then
    echo "Revision $GIT_ISH was already uploaded; exiting"
    exit 1
  fi
fi

sudo chown $USER: -R artifacts/

mkdir "$DEST"
mkdir "$GIT_ISH"

cp nix-installer.sh "$DEST"/
cp nix-installer.sh "$GIT_ISH"/

for artifact in $(find artifacts/ -type f); do
  chmod +x "$artifact"
  cp "$artifact" "$DEST"/
  cp "$artifact" "$GIT_ISH"/
done

sed -i "s@https://install.determinate.systems/nix@$DEST_INSTALL_URL@" "$DEST/nix-installer.sh"
sed -i "s@https://install.determinate.systems/nix@https://install.determinate.systems/nix/rev/$GIT_ISH@" "$GIT_ISH/nix-installer.sh"

if is_tag; then
  cp "$DEST/nix-installer.sh" ./nix-installer.sh
fi

# If any artifact already exists in S3 and the hash is the same, we don't want to reupload
check_reupload() {
  dest="$1"

  for file in $(find "$dest" -type f); do
    artifact_path="$dest"/"$(basename "$file")"
    md5="$(md5sum "$file" | cut -d' ' -f1)"
    obj="$(aws s3api head-object --bucket "$AWS_BUCKET" --key "$artifact_path" || echo '{}')"
    obj_md5="$(jq -r .ETag <<<"$obj" | jq -r)" # head-object call returns ETag quoted, so `jq -r` again to unquote it

    # Object doesn't exist, so let's check the next one
    if [[ "$obj_md5" == "null" ]]; then
      continue
    fi

    if [[ "$md5" != "$obj_md5" ]]; then
      echo "Artifact $artifact was already uploaded; exiting"
      # If we already uploaded to a tag, that's probably bad
      is_tag && exit 1 || exit 0
    fi
  done
}

check_reupload "$DEST"
if ! is_tag; then
  check_reupload "$GIT_ISH"
fi

sync_args=(--acl public-read)

# NOTE(cole-h): never allow reuploading to a tag
if is_tag; then
  sync_args+=(--if-none-match '*')
fi

# NOTE(cole-h): never allow reuploading to a rev
if ! is_tag; then
  find "$GIT_ISH/" -type f -print0 |
    while IFS= read -r -d '' artifact; do
      aws s3api put-object --bucket "$AWS_BUCKET" --key "$artifact" --body "$artifact" "${sync_args[@]}" --if-none-match '*'
    done
fi

find "$DEST/" -type f -print0 |
  while IFS= read -r -d '' artifact; do
    aws s3api put-object --bucket "$AWS_BUCKET" --key "$artifact" --body "$artifact" "${sync_args[@]}"
  done
