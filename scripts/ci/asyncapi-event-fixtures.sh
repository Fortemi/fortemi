#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

usage() {
  echo "usage: $0 generate|check" >&2
  exit 2
}

case "${1:-}" in
  generate|check)
    cargo run --quiet --manifest-path "$root/Cargo.toml" -p matric-core --bin asyncapi-event-fixtures -- "$1" "$root"
    ;;
  *)
    usage
    ;;
esac
