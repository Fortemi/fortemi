#!/usr/bin/env bash
# Reject provider-specific realtime imports outside adapter modules.

set -euo pipefail

ROOT="${1:-.}"
FAILED=0

while IFS= read -r -d '' file; do
    case "$file" in
        */adapters/*) continue ;;
    esac

    if grep -nE '^[[:space:]]*(use|pub[[:space:]]+use)[[:space:]]+(twilio|livekit|sip)::' "$file"; then
        echo "provider-specific realtime import outside adapters/: $file" >&2
        FAILED=1
    fi
done < <(find "$ROOT/crates" -type f -name '*.rs' -print0)

exit "$FAILED"
