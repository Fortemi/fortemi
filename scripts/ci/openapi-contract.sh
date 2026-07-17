#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_dir="$root/contracts/openapi"
artifact="$artifact_dir/openapi.yaml"
checksum="$artifact_dir/openapi.sha256"

usage() {
  echo "usage: $0 generate|check|check-file <candidate>|receipt <candidate> <output>" >&2
  exit 2
}

verify_checksum() {
  (cd "$artifact_dir" && sha256sum --check openapi.sha256)
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
      --export-openapi "$artifact"
    (cd "$artifact_dir" && sha256sum openapi.yaml >openapi.sha256)
    ;;
  check)
    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    cargo run --quiet --manifest-path "$root/Cargo.toml" -p matric-api -- \
      --export-openapi "$tmp"
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
    sha256="$(cut -d ' ' -f 1 "$checksum")"
    cat >"$3" <<EOF
{
  "schema_version": 1,
  "producer_repository": "Fortemi/Fortemi",
  "producer_commit": "$GITHUB_SHA",
  "contract_revision": "1",
  "contract_version": "2026.2.9",
  "artifact_path": "contracts/openapi/openapi.yaml",
  "sha256": "$sha256"
}
EOF
    ;;
  *)
    usage
    ;;
esac
