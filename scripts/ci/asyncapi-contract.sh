#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_dir="$root/contracts/asyncapi"
artifact="$artifact_dir/asyncapi.yaml"
checksum="$artifact_dir/asyncapi.sha256"

usage() {
  echo "usage: $0 generate|check|check-file <candidate>|receipt <candidate> <output>" >&2
  exit 2
}

file_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

verify_checksum() {
  [[ "$(file_sha256 "$artifact")" == "$(cut -d ' ' -f 1 "$checksum")" ]]
}

write_checksum() {
  printf '%s  asyncapi.yaml\n' "$(file_sha256 "$artifact")" >"$checksum"
}

check_candidate() {
  local candidate="$1"
  verify_checksum
  cmp -- "$artifact" "$candidate"
}

case "${1:-}" in
  generate)
    mkdir -p "$artifact_dir"
    cargo run --quiet --manifest-path "$root/Cargo.toml" -p matric-api -- \
      --export-asyncapi "$artifact"
    write_checksum
    ;;
  check)
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    cargo run --quiet --manifest-path "$root/Cargo.toml" -p matric-api -- \
      --export-asyncapi "$tmp"
    check_candidate "$tmp"
    ;;
  check-file)
    [[ $# -eq 2 ]] || usage
    check_candidate "$2"
    ;;
  receipt)
    [[ $# -eq 3 ]] || usage
    [[ "${GITHUB_SHA:-}" =~ ^[0-9a-fA-F]{40,64}$ ]] || {
      echo "GITHUB_SHA must contain the exact producer commit" >&2
      exit 1
    }
    check_candidate "$2"
    cat >"$3" <<EOF
{
  "schema_version": 1,
  "producer_repository": "Fortemi/fortemi",
  "producer_commit": "$GITHUB_SHA",
  "contract_revision": "1",
  "artifact_path": "contracts/asyncapi/asyncapi.yaml",
  "sha256": "$(file_sha256 "$artifact")"
}
EOF
    ;;
  *)
    usage
    ;;
esac
