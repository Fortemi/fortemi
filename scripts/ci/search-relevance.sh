#!/usr/bin/env bash
# Deterministic keyword relevance; requires an explicitly selected disposable DB.
set -euo pipefail
: "${DATABASE_URL:?DATABASE_URL must select a disposable PostgreSQL 18+ database}"
export CARGO_BUILD_JOBS=6
export FORTEMI_RELEVANCE_REPORT="${FORTEMI_RELEVANCE_REPORT:-target/relevance/keyword.json}"
mkdir -p "$(dirname "$FORTEMI_RELEVANCE_REPORT")"
cargo test --locked -j 6 -p matric-search --test adaptive_query_test \
  -- --test-threads=1 --nocapture
python3 scripts/ci/search-relevance.py --report "$FORTEMI_RELEVANCE_REPORT"
python3 scripts/ci/test_search_relevance.py --report "$FORTEMI_RELEVANCE_REPORT"
