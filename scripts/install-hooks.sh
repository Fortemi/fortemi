#!/bin/bash
# Install git hooks for Matric Memory development

set -e

# Get the repository root
REPO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null || echo ".")
HOOKS_DIR="${REPO_ROOT}/.git/hooks"
SCRIPTS_DIR="${REPO_ROOT}/scripts"

echo "Installing git hooks..."

# Install pre-commit hook
if [ -f "${HOOKS_DIR}/pre-commit" ]; then
    echo "Warning: pre-commit hook already exists, creating backup..."
    cp "${HOOKS_DIR}/pre-commit" "${HOOKS_DIR}/pre-commit.backup"
fi

cat > "${HOOKS_DIR}/pre-commit" << 'EOF'
#!/bin/bash
# Git pre-commit hook for Matric Memory
# This hook runs cargo fmt and clippy checks before allowing commits

# Get the repository root directory
REPO_ROOT=$(git rev-parse --show-toplevel)

# Run the pre-commit script
exec "${REPO_ROOT}/scripts/pre-commit.sh"
EOF

chmod +x "${HOOKS_DIR}/pre-commit"
echo "✓ Installed pre-commit hook"

echo ""
echo "Git hooks installed successfully!"
echo ""
echo "The pre-commit hook will now run cargo fmt and clippy checks"
echo "before each commit to ensure code quality."
echo ""
echo "To skip the hook (not recommended), use: git commit --no-verify"
