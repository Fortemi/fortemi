#!/usr/bin/env bash
# Git core.sshCommand router: Gitea via the OpenBao deploy key, GitHub via the operator's SSH keys.

set -euo pipefail
set +x

die() { printf 'ssh-route: %s\n' "$*" >&2; exit 1; }

# Git probes SSH variant support with -G; answer without resolving secrets.
[[ "${1:-}" == -G ]] && exit 0

vault() { git config --get "aiwg.vault.$1" || die "aiwg.vault.$1 is not configured"; }

GITEA_HOST="$(vault sshExpectedHost)"
GITHUB_HOST="${FORTEMI_GITHUB_SSH_HOST:-github.com}"

target=""
for arg in "$@"; do
  case "$arg" in
    "git@$GITEA_HOST") target=gitea ;;
    "git@$GITHUB_HOST") target=github ;;
  esac
done

case "$target" in
  gitea)
    exec env \
      OPENBAO_GIT_APPROLE="$(vault readerRole)" \
      OPENBAO_GIT_DATA_PATH="$(vault sshKeyPath)" \
      OPENBAO_GIT_FIELD=private_key \
      OPENBAO_GIT_FINGERPRINT="$(vault sshKeyFingerprint)" \
      OPENBAO_GIT_EXPECTED_HOST="$GITEA_HOST" \
      OPENBAO_GIT_EXPECTED_REPO="$(vault sshExpectedRepo)" \
      BAO_CACERT="$(vault caCert)" \
      "$(vault sshHelper)" "$@"
    ;;
  github)
    exec ssh -o BatchMode=yes "$@"
    ;;
  *)
    die "SSH target is neither git@$GITEA_HOST nor git@$GITHUB_HOST"
    ;;
esac
