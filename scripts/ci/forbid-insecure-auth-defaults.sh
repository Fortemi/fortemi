#!/usr/bin/env bash
set -euo pipefail

root="${1:-.}"
cd "$root"

fail=0
search_error=0

if command -v rg >/dev/null 2>&1; then
  search_tool=rg
elif command -v grep >/dev/null 2>&1; then
  search_tool=grep
else
  echo "ERROR: insecure-auth guard requires rg or grep" >&2
  exit 2
fi

run_search() {
  local pattern="$1"
  shift
  if [[ "$search_tool" == rg ]]; then
    rg -n \
      --glob '!docker-compose.workstation.yml' \
      --glob '!WORKSTATION-SETUP.md' \
      --glob '!scripts/ci/rebuild-shard-in-ci.sh' \
      --glob '!docs/architecture/adr/ADR-071-auth-middleware.md' \
      -- "$pattern" "$@"
  else
    grep -rInE \
      --exclude='docker-compose.workstation.yml' \
      --exclude='WORKSTATION-SETUP.md' \
      --exclude='rebuild-shard-in-ci.sh' \
      --exclude='ADR-071-auth-middleware.md' \
      -- "$pattern" "$@"
  fi
}

check_no_match() {
  local description="$1"
  local pattern="$2"
  shift 2
  local matches status
  set +e
  matches=$(run_search "$pattern" "$@")
  status=$?
  set -e
  case "$status" in
    0)
      echo "ERROR: ${description}" >&2
      echo "$matches" >&2
      fail=1
      ;;
    1)
      ;;
    *)
      echo "ERROR: ${description}: ${search_tool} exited ${status}" >&2
      search_error=1
      ;;
  esac
}

check_no_match \
  "production compose/installer files must not default REQUIRE_AUTH to false" \
  'REQUIRE_AUTH=\$\{REQUIRE_AUTH:-false\}|^REQUIRE_AUTH=false$' \
  docker-compose*.yml installer scripts

check_no_match \
  "operator docs must not describe REQUIRE_AUTH=false as the default" \
  'REQUIRE_AUTH.*default[^`\n]*false|default[^`\n]*REQUIRE_AUTH=false|When `REQUIRE_AUTH=false` \(default\)|disabled \(default\)' \
  README.md docs/content CLAUDE.md

check_no_match \
  "handlers must not branch on optional auth presence" \
  '\bauth\.is_some\(' \
  crates

if [[ "$search_error" -ne 0 ]]; then
  exit 2
fi
echo "Insecure auth default guard passed (search_tool=${search_tool})"
exit "$fail"
