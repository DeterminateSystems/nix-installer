set -eu

# If the revision directory has already been created in S3 somehow, we don't want to reupload
if aws s3 ls "$AWS_BUCKET"/"$GIT_ISH"/; then
  echo "Revision $GIT_ISH was already uploaded; exiting"
  exit 1
fi

sudo chown $USER: -R artifacts/

DEST="$1"
GIT_ISH="$2"

mkdir "$GIT_ISH"

sed -i "s@https://install.determinate.systems/nix@https://install.determinate.systems/nix/rev/$GIT_ISH@" nix-installer.sh
cp nix-installer.sh "$GIT_ISH"/

for artifact in $(find artifacts/ -type f); do
  chmod +x "$artifact"
  cp "$artifact" "$GIT_ISH"/
done

# If any artifact already exists in S3 and the hash is the same, we don't want to reupload
for file in $(find "$GIT_ISH" -type f); do
  artifact_path="$DEST"/"$(basename "$artifact")"
  md5="$(md5sum "$artifact" | cut -d' ' -f1)"
  obj="$(aws s3api head-object --bucket "$AWS_BUCKET" --key "$artifact_path" || echo '{}')"
  obj_md5="$(jq -r .ETag <<<"$obj" | jq -r)" # head-object call returns ETag quoted, so `jq -r` again to unquote it

  if [[ "$md5" == "$obj_md5" ]]; then
    echo "Artifact $artifact was already uploaded; exiting"
    exit 0
  fi
done

aws s3 sync "$GIT_ISH"/ s3://"$AWS_BUCKET"/"$GIT_ISH"/ --acl public-read
aws s3 sync s3://"$AWS_BUCKET"/"$GIT_ISH"/ s3://"$AWS_BUCKET"/"$DEST"/ --acl public-read
