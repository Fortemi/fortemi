#!/usr/bin/env bash
# Cut a full stable Fortemi release tag with the OpenBao-custodied release key.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

RELEASE_FINGERPRINT="9292EFCBB0EA41BECEEFDAFA9C1B8CE0E0E09C33"
RELEASE_APPROLE="${FORTEMI_RELEASE_APPROLE:-ci-fortemi}"
VERSION="${1:-}"
shift || true
TAG_MESSAGE=""
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    -m|--message)
      [[ $# -ge 2 ]] || { echo "FAIL: $1 requires a value." >&2; exit 2; }
      TAG_MESSAGE="$2"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    *)
      echo "FAIL: unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

[[ "$VERSION" =~ ^[0-9]{4}\.([1-9]|1[0-2])\.([0-9]|[1-9][0-9]+)$ ]] || {
  echo "Usage: $0 YYYY.M.PATCH [-m message] [--dry-run]" >&2
  echo "FAIL: only full stable CalVer releases are accepted." >&2
  exit 2
}
TAG="v$VERSION"

workspace_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)"
mcp_version="$(node -e "console.log(require('./mcp-server/package.json').version)")"
[[ "$workspace_version" == "$VERSION" ]] || {
  echo "FAIL: Cargo workspace version is '$workspace_version', expected '$VERSION'." >&2
  exit 1
}
[[ "$mcp_version" == "$VERSION" ]] || {
  echo "FAIL: MCP package version is '$mcp_version', expected '$VERSION'." >&2
  exit 1
}
grep -q "^## \[$VERSION\]" CHANGELOG.md || {
  echo "FAIL: CHANGELOG.md has no $VERSION release entry." >&2
  exit 1
}
[[ -f "docs/releases/v$VERSION-announcement.md" ]] || {
  echo "FAIL: docs/releases/v$VERSION-announcement.md is missing." >&2
  exit 1
}
[[ -z "$(git status --porcelain)" ]] || {
  echo "FAIL: release tags require a clean worktree." >&2
  exit 1
}
git fetch --quiet origin main
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || {
  echo "FAIL: HEAD is not the current origin/main commit." >&2
  exit 1
}
if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "FAIL: tag $TAG already exists." >&2
  exit 1
fi

VERIFY_HOME="$(mktemp -d /dev/shm/fortemi-public-keys.XXXXXX)"
chmod 700 "$VERIFY_HOME"
cleanup() {
  gpgconf --homedir "$VERIFY_HOME" --kill gpg-agent >/dev/null 2>&1 || true
  rm -rf "$VERIFY_HOME"
}
trap cleanup EXIT INT TERM
GNUPGHOME="$VERIFY_HOME" gpg --batch --import .gitea/keys/maintainers.asc >/dev/null 2>&1
GNUPGHOME="$VERIFY_HOME" gpg --batch --with-colons --list-keys "$RELEASE_FINGERPRINT" \
  | awk -F: '$1 == "fpr" { print $10 }' | grep -qx "$RELEASE_FINGERPRINT" || {
    echo "FAIL: release public key is not published in .gitea/keys/maintainers.asc." >&2
    exit 1
  }

if [[ "$DRY_RUN" == 1 ]]; then
  probe="$VERIFY_HOME/signing-probe"
  printf 'Fortemi release signing probe\n' >"$probe"
  FORTEMI_GPG_PURPOSE=release FORTEMI_OPENBAO_APPROLE="$RELEASE_APPROLE" \
    tools/git/gpg-from-openbao.sh --yes --detach-sign --output "$probe.sig" "$probe"
  echo "OpenBao release signing dry-run passed for $TAG."
  exit 0
fi

[[ -n "$TAG_MESSAGE" ]] || TAG_MESSAGE="$TAG"
FORTEMI_GPG_PURPOSE=release FORTEMI_OPENBAO_APPROLE="$RELEASE_APPROLE" \
  git -c gpg.program="$ROOT/tools/git/gpg-from-openbao.sh" \
  tag -s -u "$RELEASE_FINGERPRINT" "$TAG" -m "$TAG_MESSAGE"

if ! tools/ci/verify-signed-tag.sh "$TAG"; then
  git tag -d "$TAG" >/dev/null 2>&1 || true
  echo "FAIL: local signature verification failed; $TAG was removed." >&2
  exit 1
fi

echo "Signed and verified $TAG with release key $RELEASE_FINGERPRINT."
echo "Next: git push origin $TAG"
