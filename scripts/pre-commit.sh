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
echo -e "\n${YELLOW}[1/3] Validating MCP tool schemas...${NC}"
if node mcp-server/validate-schemas.cjs > /dev/null 2>&1; then
    echo -e "${GREEN}✓ MCP tool schemas valid (draft 2020-12)${NC}"
else
    echo -e "${RED}✗ MCP tool schema validation failed${NC}"
    echo -e "${YELLOW}Run 'node mcp-server/validate-schemas.cjs' for details${NC}"
    FAILED=1
fi

# Check 2: cargo fmt
echo -e "\n${YELLOW}[2/3] Checking code formatting...${NC}"
if cargo fmt --check --all; then
    echo -e "${GREEN}✓ Code formatting is correct${NC}"
else
    echo -e "${RED}✗ Code formatting issues found${NC}"
    echo -e "${YELLOW}Run 'cargo fmt --all' to fix formatting${NC}"
    FAILED=1
fi

# Check 3: cargo clippy
echo -e "\n${YELLOW}[3/3] Running clippy lints...${NC}"
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
