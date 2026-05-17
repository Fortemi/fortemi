#!/bin/bash
# Pre-commit hook for Matric Memory
# Runs cargo fmt and clippy checks before allowing commits

set -e

echo "Running pre-commit checks..."

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track if any checks fail
FAILED=0

# Check 1: MCP tool schema validation (fast — catches broken schemas before deploy)
echo -e "\n${YELLOW}[1/4] Validating MCP tool schemas...${NC}"
if node mcp-server/validate-schemas.cjs > /dev/null 2>&1; then
    echo -e "${GREEN}✓ MCP tool schemas valid (draft 2020-12)${NC}"
else
    echo -e "${RED}✗ MCP tool schema validation failed${NC}"
    echo -e "${YELLOW}Run 'node mcp-server/validate-schemas.cjs' for details${NC}"
    FAILED=1
fi

# Check 2: mcp-server lockfile sync (only when mcp-server files staged)
# Catches the drift mode that produced #686 — avoids surprising npm audit
# failures landing for unrelated reasons.
if git diff --cached --name-only | grep -qE '^mcp-server/(package\.json|package-lock\.json)$'; then
    echo -e "\n${YELLOW}[2a/4] mcp-server lockfile sync check...${NC}"
    if (cd mcp-server && npm ci --ignore-scripts --no-fund --no-audit > /dev/null 2>&1); then
        echo -e "${GREEN}✓ mcp-server/package-lock.json is in sync${NC}"
    else
        echo -e "${RED}✗ mcp-server/package-lock.json out of sync with package.json${NC}"
        echo -e "${YELLOW}Run 'cd mcp-server && npm install' to refresh the lockfile${NC}"
        FAILED=1
    fi
fi

# Check 3: cargo fmt
echo -e "\n${YELLOW}[3/4] Checking code formatting...${NC}"
if cargo fmt --check --all; then
    echo -e "${GREEN}✓ Code formatting is correct${NC}"
else
    echo -e "${RED}✗ Code formatting issues found${NC}"
    echo -e "${YELLOW}Run 'cargo fmt --all' to fix formatting${NC}"
    FAILED=1
fi

# Check 4: cargo clippy
echo -e "\n${YELLOW}[4/4] Running clippy lints...${NC}"
if cargo clippy --all-targets --all-features -- -D warnings; then
    echo -e "${GREEN}✓ No clippy warnings${NC}"
else
    echo -e "${RED}✗ Clippy warnings found${NC}"
    echo -e "${YELLOW}Fix the clippy warnings above before committing${NC}"
    FAILED=1
fi

# Final result
echo ""
if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}All pre-commit checks passed!${NC}"
    exit 0
else
    echo -e "${RED}Pre-commit checks failed. Please fix the issues above.${NC}"
    echo -e "${YELLOW}To skip these checks (not recommended), use: git commit --no-verify${NC}"
    exit 1
fi
