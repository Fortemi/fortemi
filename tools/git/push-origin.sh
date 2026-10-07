#!/usr/bin/env bash
# Push Fortemi to Gitea through its project-dedicated OpenBao deploy key.

set -euo pipefail
set +x

ROOT="$(git rev-parse --show-toplevel)"
ROUTER="$ROOT/tools/git/ssh-route.sh"
REPO="$(git -C "$ROOT" config --get aiwg.vault.sshExpectedRepo)"

[[ -x "$ROUTER" && -n "$REPO" ]] || { echo 'FAIL: Fortemi Git vault routing is not configured.' >&2; exit 1; }
[[ "$(git -C "$ROOT" config --get core.sshCommand)" == *ssh-route.sh* ]] \
  || { echo 'FAIL: core.sshCommand does not use tools/git/ssh-route.sh.' >&2; exit 1; }

# A read through the router proves the vault key resolves, matches its pinned
# fingerprint and is accepted by Gitea for this repository.
git -C "$ROOT" ls-remote --exit-code origin HEAD >/dev/null \
  || { echo "FAIL: Gitea rejected the vault deploy key for $REPO." >&2; exit 1; }
if [[ "${1:-}" == --check ]]; then
  echo "Gitea SSH authentication passed for $REPO via the vault deploy key."
  exit 0
fi
git -C "$ROOT" push origin "$@"
