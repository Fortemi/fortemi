#!/usr/bin/env bash
# Git gpg.program adapter backed by a TPM-sealed, least-privilege OpenBao AppRole.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
PURPOSE="${FORTEMI_GPG_PURPOSE:-commit}"

case "$PURPOSE" in
  commit)
    FINGERPRINT="62297562B1C7053088F405DB0117DAAA677A5BF2"
    APPROLE="${FORTEMI_OPENBAO_APPROLE:-git-fortemi-roctinam}"
    SPEC="$ROOT/ci/vault-fetch.commit-signing.spec"
    ;;
  release)
    FINGERPRINT="9292EFCBB0EA41BECEEFDAFA9C1B8CE0E0E09C33"
    APPROLE="${FORTEMI_OPENBAO_APPROLE:-ci-fortemi}"
    SPEC="$ROOT/ci/vault-fetch.release-signing.spec"
    ;;
  *)
    echo "FAIL: FORTEMI_GPG_PURPOSE must be commit or release." >&2
    exit 2
    ;;
esac

for command in curl gpg jq stat sudo systemd-creds; do
  command -v "$command" >/dev/null 2>&1 || {
    echo "FAIL: required command is unavailable: $command" >&2
    exit 1
  }
done

CREDSTORE="${FORTEMI_CREDSTORE_DIR:-/etc/credstore.encrypted}"
ROLE_CRED="$CREDSTORE/openbao-$APPROLE-role-id"
SECRET_CRED="$CREDSTORE/openbao-$APPROLE-secret-id"
sudo -n test -f "$ROLE_CRED" && sudo -n test -f "$SECRET_CRED" || {
  echo "FAIL: TPM AppRole pair is unavailable for '$APPROLE'." >&2
  exit 1
}

VAULT_CI_ROLE_ID="$(sudo -n systemd-creds decrypt "$ROLE_CRED" - 2>/dev/null)"
VAULT_CI_SECRET_ID="$(sudo -n systemd-creds decrypt "$SECRET_CRED" - 2>/dev/null)"
[[ -n "$VAULT_CI_ROLE_ID" && -n "$VAULT_CI_SECRET_ID" ]] || {
  echo "FAIL: TPM AppRole pair for '$APPROLE' could not be decrypted." >&2
  exit 1
}
export VAULT_CI_ROLE_ID VAULT_CI_SECRET_ID
export VAULT_ADDR="${VAULT_ADDR:-https://rca-g2.s9.internal:8200}"
export VAULT_CACERT="${VAULT_CACERT:-$ROOT/ci/trust/integro-labs-root-ca-g2.crt}"
export VAULT_SKIP_VERIFY=0

RUNTIME_PARENT=""
for candidate in "${XDG_RUNTIME_DIR:-}" /dev/shm; do
  if [[ -n "$candidate" && -d "$candidate" && -w "$candidate" && "$(stat -f -c %T "$candidate" 2>/dev/null || true)" == tmpfs ]]; then
    RUNTIME_PARENT="$candidate"
    break
  fi
done
[[ -n "$RUNTIME_PARENT" ]] || {
  echo "FAIL: a writable tmpfs is required for signing material." >&2
  exit 1
}

TMP="$(mktemp -d "$RUNTIME_PARENT/fortemi-gpg.XXXXXX")"
GNUPGHOME="$TMP/gnupg"
ENV_FILE="$TMP/fetched.env"
cleanup() {
  RUNNER_TEMP="$TMP" bash "$ROOT/ci/vault-fetch.sh" --cleanup >/dev/null 2>&1 || true
  gpgconf --homedir "$GNUPGHOME" --kill gpg-agent >/dev/null 2>&1 || true
  unset VAULT_CI_ROLE_ID VAULT_CI_SECRET_ID
  rm -rf "$TMP"
}
trap cleanup EXIT INT TERM
mkdir -m 700 "$GNUPGHOME"
touch "$ENV_FILE"
chmod 600 "$ENV_FILE"

GNUPGHOME="$GNUPGHOME" gpg --batch --with-colons --show-keys \
  "$ROOT/.gitea/keys/maintainers.asc" 2>/dev/null |
  awk -F: '$1 == "fpr" { print $10 }' | grep -qx "$FINGERPRINT" || {
    echo "FAIL: $PURPOSE signing authority is not published in the repository keyring." >&2
    exit 1
  }

RUNNER_TEMP="$TMP" bash "$ROOT/ci/vault-fetch.sh" --spec "$SPEC" --env-file "$ENV_FILE" >/dev/null
# The fetch file contains mode-600 paths, never secret values.
# shellcheck disable=SC1090
. "$ENV_FILE"

IMPORTED_FINGERPRINT="$(
  GNUPGHOME="$GNUPGHOME" gpg --batch --with-colons --import-options show-only \
    --import "$GPG_PRIVATE_KEY_FILE" 2>/dev/null |
    awk -F: '$1 == "fpr" { print $10; exit }'
)"
[[ "$IMPORTED_FINGERPRINT" == "$FINGERPRINT" ]] || {
  echo "FAIL: OpenBao returned the wrong $PURPOSE signing key." >&2
  exit 1
}

GNUPGHOME="$GNUPGHOME" gpg --batch --import "$GPG_PRIVATE_KEY_FILE" >/dev/null 2>&1
GPG_ARGS=(--batch --pinentry-mode loopback)
if [[ -n "${GPG_PASSPHRASE_FILE:-}" ]]; then
  GPG_ARGS+=(--passphrase-file "$GPG_PASSPHRASE_FILE")
fi
set +e
GNUPGHOME="$GNUPGHOME" gpg "${GPG_ARGS[@]}" "$@"
status=$?
set -e
exit "$status"
