#!/bin/bash
# scripts/check-deps.sh
#
# Workspace-level dependency check covering both the Rust workspace and the
# mcp-server npm subproject. Runs the same checks CI runs (cargo audit, cargo
# deny, npm ci sync verification, npm audit) so developers can validate
# dependency state locally before pushing.
#
# Usage:
#   ./scripts/check-deps.sh          # full check
#   ./scripts/check-deps.sh --fast   # skip cargo-audit (which fetches the DB)
#
# Exit codes:
#   0 — all checks pass
#   1 — at least one check failed
#   2 — required tooling missing

set -e

FAST=0
if [ "${1:-}" = "--fast" ]; then
    FAST=1
fi

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

FAILED=0
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo -e "${RED}✗ Required tool missing: $1${NC}"
        echo "  $2"
        exit 2
    fi
}

require cargo "Install Rust: https://rustup.rs/"
require npm "Install Node.js 18+: https://nodejs.org/"

echo -e "${YELLOW}[1/4] Rust workspace: cargo check${NC}"
if cargo check --workspace --quiet 2>&1 | tail -3; then
    echo -e "${GREEN}✓ cargo check passed${NC}"
else
    echo -e "${RED}✗ cargo check failed${NC}"
    FAILED=1
fi

if [ "$FAST" = "0" ]; then
    echo -e "\n${YELLOW}[2/4] Rust workspace: cargo audit${NC}"
    if command -v cargo-audit >/dev/null 2>&1; then
        if cargo audit; then
            echo -e "${GREEN}✓ cargo audit passed${NC}"
        else
            echo -e "${RED}✗ cargo audit found vulnerabilities${NC}"
            FAILED=1
        fi
    else
        echo -e "${YELLOW}⚠ cargo-audit not installed — skipping. Install: cargo install --locked cargo-audit${NC}"
    fi

    echo -e "\n${YELLOW}[3/4] Rust workspace: cargo deny${NC}"
    if command -v cargo-deny >/dev/null 2>&1; then
        if cargo deny check; then
            echo -e "${GREEN}✓ cargo deny passed${NC}"
        else
            echo -e "${RED}✗ cargo deny failed${NC}"
            FAILED=1
        fi
    else
        echo -e "${YELLOW}⚠ cargo-deny not installed — skipping. Install: cargo install --locked cargo-deny${NC}"
    fi
else
    echo -e "${YELLOW}[2-3/4] Skipped Rust audit/deny (--fast mode)${NC}"
fi

echo -e "\n${YELLOW}[4/4] mcp-server: npm ci sync + npm audit${NC}"
(
    cd mcp-server
    if npm ci --ignore-scripts --no-fund --no-audit > /tmp/npm-ci-output 2>&1; then
        echo -e "${GREEN}✓ mcp-server/package-lock.json is in sync${NC}"
    else
        echo -e "${RED}✗ mcp-server/package-lock.json out of sync with package.json${NC}"
        tail -10 /tmp/npm-ci-output
        FAILED=1
    fi
    if npm audit --audit-level=high > /tmp/npm-audit-output 2>&1; then
        echo -e "${GREEN}✓ mcp-server npm audit (high+) passed${NC}"
    else
        echo -e "${RED}✗ mcp-server has high/critical npm vulnerabilities${NC}"
        tail -10 /tmp/npm-audit-output
        FAILED=1
    fi
)

echo ""
if [ "$FAILED" = "0" ]; then
    echo -e "${GREEN}✓ All dependency checks passed${NC}"
    exit 0
else
    echo -e "${RED}✗ One or more dependency checks failed${NC}"
    exit 1
fi
