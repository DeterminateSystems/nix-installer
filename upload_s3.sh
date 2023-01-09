set -eu

# If the revision directory has already been created in S3 somehow, we don't want to reupload
if aws s3 ls "$AWS_BUCKET"/"$GITHUB_SHA"/; then
  echo "Revision $GITHUB_SHA was already uploaded; exiting"
  exit 1
fi

sudo chown $USER: -R artifacts/

DEST="$1"

mkdir "$GITHUB_SHA"

sed -i "s@https://install.determinate.systems/nix@https://install.determinate.systems/nix/rev/$GITHUB_SHA@" nix-installer.sh
cp nix-installer.sh "$GITHUB_SHA"/

for artifact in $(find artifacts/ -type f); do
  chmod +x "$artifact"
  cp "$artifact" "$GITHUB_SHA"/
done

# If any artifact already exists in S3 and the hash is the same, we don't want to reupload
for file in $(find "$GITHUB_SHA" -type f); do
  artifact_path="$DEST"/"$(basename "$artifact")"
  md5="$(md5sum "$artifact" | cut -d' ' -f1)"
  obj="$(aws s3api head-object --bucket "$AWS_BUCKET" --key "$artifact_path")"
  obj_md5="$(jq -r .ETag <<<"$obj" | jq -r)" # head-object call returns ETag quoted, so `jq -r` again to unquote it

  if [[ "$md5" == "$obj_md5" ]]; then
    echo "Artifact $artifact was already uploaded; exiting"
    exit 0
  fi
done

aws s3 sync "$GITHUB_SHA"/ s3://"$AWS_BUCKET"/"$GITHUB_SHA"/ --acl public-read
aws s3 sync s3://"$AWS_BUCKET"/"$GITHUB_SHA"/ s3://"$AWS_BUCKET"/"$DEST"/ --acl public-read
