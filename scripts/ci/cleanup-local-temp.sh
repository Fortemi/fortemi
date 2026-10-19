#!/usr/bin/env bash
# CI temporaries belong to the registered run root, removed only in quarantine.
set -euo pipefail
if [[ -n "${CI_RESOURCE_SCOPE:-}" ]]; then
  echo '::notice::Retained CI temporary directory for verified final quarantine reconciliation.'
else
  rm -rf -- "$@"
fi
