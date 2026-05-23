#!/usr/bin/env bash
set -euo pipefail

root="${1:-.}"
cd "$root"

fail=0

check_no_match() {
  local description="$1"
  local pattern="$2"
  shift 2
  local matches
  matches=$(rg -n --glob '!docker-compose.workstation.yml' --glob '!WORKSTATION-SETUP.md' --glob '!scripts/ci/rebuild-shard-in-ci.sh' --glob '!docs/architecture/adr/ADR-071-auth-middleware.md' "$pattern" "$@" || true)
  if [[ -n "$matches" ]]; then
    echo "ERROR: ${description}" >&2
    echo "$matches" >&2
    fail=1
  fi
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

exit "$fail"
