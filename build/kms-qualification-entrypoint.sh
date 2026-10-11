#!/bin/sh
# Entrypoint of the KMS qualification image (build/Dockerfile.kms-qualification).
# Runs the one qualification test and fails unless exactly that test ran and
# passed: libtest exits 0 when a filter matches nothing, which would otherwise
# read as a pass. Writes nothing to disk (the Job's root filesystem is read-only).
set -eu
rc=0
out="$(/app/kms-qualification --ignored --exact live_aws_kms_workload_identity \
  --nocapture --test-threads=1 2>&1)" || rc=$?
printf '%s\n' "$out"
[ "$rc" -eq 0 ] || exit "$rc"
if ! printf '%s\n' "$out" | grep -q '^test result: ok\. 1 passed; 0 failed'; then
  echo "kms-qualification: expected exactly one passing test" >&2
  exit 1
fi
