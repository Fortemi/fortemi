#!/usr/bin/env bash
# Deterministic keyword relevance; requires an explicitly selected disposable DB.
set -euo pipefail
: "${DATABASE_URL:?DATABASE_URL must select a disposable PostgreSQL 18+ database}"
export CARGO_BUILD_JOBS=6
# Cargo runs integration tests from the crate directory, so the report path the
# test writes must be absolute; resolve it here before invoking Cargo.
report="${FORTEMI_RELEVANCE_REPORT:-${CARGO_TARGET_DIR:-$PWD/target}/relevance/keyword.json}"
case "$report" in /*) ;; *) report="$PWD/$report" ;; esac
export FORTEMI_RELEVANCE_REPORT="$report"
mkdir -p "$(dirname "$FORTEMI_RELEVANCE_REPORT")"
rm -f "$FORTEMI_RELEVANCE_REPORT"
cargo test --locked -j 6 -p matric-search --test adaptive_query_test \
  -- --test-threads=1 --nocapture
python3 scripts/ci/search-relevance.py --report "$FORTEMI_RELEVANCE_REPORT"
python3 scripts/ci/test_search_relevance.py --report "$FORTEMI_RELEVANCE_REPORT"
