#!/usr/bin/env bash
# Verify a stable release tag against Fortemi's published release-key fingerprint.

set -euo pipefail

TAG="${1:-${RELEASE_TAG:-${GITHUB_REF_NAME:-}}}"
FINGERPRINT="9292EFCBB0EA41BECEEFDAFA9C1B8CE0E0E09C33"
[[ "$TAG" =~ ^v[0-9]{4}\.([1-9]|1[0-2])\.([0-9]|[1-9][0-9]+)$ ]] || {
  echo "FAIL: '$TAG' is not a full stable release tag." >&2
  exit 1
}
[[ "$(git cat-file -t "$TAG" 2>/dev/null || true)" == "tag" ]] || {
  echo "FAIL: '$TAG' is not an annotated tag object." >&2
  exit 1
}

VERIFY_HOME="$(mktemp -d "${RUNNER_TEMP:-/dev/shm}/fortemi-tag-verify.XXXXXX")"
chmod 700 "$VERIFY_HOME"
cleanup() {
  gpgconf --homedir "$VERIFY_HOME" --kill gpg-agent >/dev/null 2>&1 || true
  rm -rf "$VERIFY_HOME"
}
trap cleanup EXIT INT TERM
GNUPGHOME="$VERIFY_HOME" gpg --batch --import .gitea/keys/maintainers.asc >/dev/null 2>&1

if ! output="$(GNUPGHOME="$VERIFY_HOME" git verify-tag --raw "$TAG" 2>&1)"; then
  echo "$output" >&2
  echo "FAIL: signature verification failed for $TAG." >&2
  exit 1
fi
actual="$(printf '%s\n' "$output" | sed -n 's/^\[GNUPG:\] VALIDSIG \([A-F0-9]\{40\}\).*/\1/p' | head -1)"
[[ "$actual" == "$FINGERPRINT" ]] || {
  echo "FAIL: $TAG was signed by '${actual:-unknown}', expected '$FINGERPRINT'." >&2
  exit 1
}
echo "Verified $TAG with release key $FINGERPRINT."
