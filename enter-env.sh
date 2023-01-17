#!/usr/bin/env nix-shell
#!nix-shell -p vault awscli2 jq -i bash
# shellcheck shell=bash

set +x # don't leak secrets!
set -eu
umask 077

scriptroot=$(dirname "$(realpath "$0")")
scratch=$(mktemp -d -t tmp.XXXXXXXXXX)

vault token lookup &>/dev/null || {
  echo "You're not logged in to vault! Exiting."
  exit 1
}

function finish {
  set +e
  rm -rf "$scratch"
  if [ "${VAULT_EXIT_ACCESSOR:-}" != "" ]; then
    if vault token lookup &>/dev/null; then
        echo "--> Revoking my token..." >&2
        vault token revoke -self
    fi
  fi
  set -e
}
trap finish EXIT

assume_role() {
  role=$1
  echo "--> Assuming role: $role" >&2
  vault_creds=$(vault token create \
    -display-name="$role" \
    -format=json \
    -role "$role")

  VAULT_EXIT_ACCESSOR=$(jq -r .auth.accessor <<<"$vault_creds")
  export VAULT_TOKEN
  VAULT_TOKEN=$(jq -r .auth.client_token <<<"$vault_creds")
}

function provision_aws_creds() {
  url="$1"
  local ok=
  echo "--> Setting AWS variables: " >&2
  echo "                       AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_SESSION_TOKEN" >&2

  aws_creds=$(vault kv get -format=json "$url")
  export AWS_ACCESS_KEY_ID
  AWS_ACCESS_KEY_ID=$(jq -r .data.access_key <<<"$aws_creds")
  export AWS_SECRET_ACCESS_KEY
  AWS_SECRET_ACCESS_KEY=$(jq -r .data.secret_key <<<"$aws_creds")
  export AWS_SESSION_TOKEN
  AWS_SESSION_TOKEN=$(jq -r .data.security_token <<<"$aws_creds")
  if [ -z "$AWS_SESSION_TOKEN" ] ||  [ "$AWS_SESSION_TOKEN" == "null" ]; then
    unset AWS_SESSION_TOKEN
  fi

  echo "--> Preflight testing the AWS credentials..." >&2
  for _ in {0..20}; do
    if check_output=$(aws sts get-caller-identity 2>&1 >/dev/null); then
        ok=1
        break
    else
        echo -n "." >&2
        sleep 1
    fi
  done
  if [[ -z "$ok" ]]; then
    echo $'\nPreflight test failed:\n'"$check_output" >&2
    return 1
  fi
  echo
  unset aws_creds
}

assume_role "internalservices_nix_installer_developer"
provision_aws_creds "internalservices/aws/creds/nix_installer"

if [ "${1:-}" == "" ]; then
    cat <<\BASH > "$scratch/bashrc"
expiration_ts=$(date +%s -d "$(vault token lookup -format=json | jq -r '.data.expire_time')")
vault_prompt() {
  local remaining=$(( $expiration_ts - $(date '+%s')))
  if [[ "$remaining" -lt 1 ]]; then
    remaining=expired
    printf '\n\e[01;33mtoken expired\e[m';
    return
  fi
  printf '\n\e[01;32mTTL:%ss\e[m' "$remaining"
}
PROMPT_COMMAND=vault_prompt
BASH

    bash --init-file "$scratch/bashrc"
else
    "$@"
fi
