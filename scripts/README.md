# Development Scripts

This directory contains development and deployment scripts for Matric Memory.

## Git Hooks

### Installation

To install the pre-commit hooks for all developers:

```bash
./scripts/install-hooks.sh
```

### Pre-commit Hook

The pre-commit hook (`pre-commit.sh`) runs automatically before each commit and performs:

1. **Code Formatting Check** - Runs `cargo fmt --check --all`
2. **Lint Check** - Runs `cargo clippy --all-targets --all-features -- -D warnings`

If either check fails, the commit is blocked.

### Manual Execution

You can run the pre-commit checks manually at any time:

```bash
./scripts/pre-commit.sh
```

### Bypassing Hooks

If you need to bypass the pre-commit hook (not recommended):

```bash
git commit --no-verify
```

### Fixing Issues

If the pre-commit hook fails:

**For formatting issues:**
```bash
cargo fmt --all
```

**For clippy warnings:**
Fix the specific warnings shown in the output. Common fixes:
```bash
# Run clippy to see all warnings
cargo clippy --all-targets --all-features

# Fix automatically where possible
cargo clippy --fix --all-targets --all-features
```

## Why Git Hooks?

Pre-commit hooks ensure:
- Consistent code formatting across all contributors
- Early detection of common issues before CI/CD
- Faster development cycle (catch issues locally vs waiting for CI)
- Better code quality and maintainability

The hooks are designed to run quickly and only check what's needed for a commit.
