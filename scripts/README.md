# Development Scripts

This directory contains development and deployment scripts for Fortémi.

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

## Demo Scripts

### Self-Index Demo

Demonstrates Fortémi indexing its own codebase for semantic code search.

**Run the demo:**
```bash
./scripts/self-index-demo.sh
```

**Prerequisites:**
- Fortémi API server running on `http://localhost:3000` (or set `FORTEMI_API_URL`)
- `curl` and `jq` installed
- Embedding service configured and running

**What it does:**
1. Creates a collection named `fortemi-codebase`
2. Indexes Rust source files with `format: "rust"`
3. Indexes TypeScript MCP server files with `format: "typescript"`
4. Indexes SQL migration files with `format: "sql"`
5. Indexes core documentation with `format: "markdown"`
6. Demonstrates semantic code search queries

**Test the demo:**
```bash
./scripts/test-self-index-demo.sh
```

This test suite validates:
- Script exists and is executable
- Required commands available (curl, jq)
- Source files exist
- Correct API endpoints used
- Proper document formats specified
- Demo search queries included
- Documentation complete

See [docs/content/self-maintenance.md](/docs/content/self-maintenance.md) for full documentation.

## Testing Scripts

### Local Test Runner (Recommended)

Use `act` to run CI workflows locally. This ensures your local tests match CI exactly.

**Install act (one-time):**
```bash
curl -s https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash
```

**Run tests locally:**
```bash
# Run the full test workflow (matches CI)
act -j fast-tests

# Run a specific job
act -j integration-tests

# List available jobs
act -l

# Run with verbose output
act -j fast-tests -v
```

**Benefits:**
- Exactly matches CI environment and workflow
- Uses same Docker containers, migrations, and test setup
- No local database setup required
- Catches CI-specific issues before pushing

### Container API Tests

Tests the API endpoints in a containerized environment:

```bash
./scripts/container-api-tests.sh
```

### Production Tests

Runs production-level validation tests:

```bash
./scripts/production-test.sh
```

### SKOS Regression Tests

Tests W3C SKOS semantic tagging functionality:

```bash
./scripts/test-skos-regression.sh
```

### Strict Search Tests

Validates strict tag filtering for data isolation:

```bash
./scripts/test-strict-search.sh
```

### MCP Tests

MCP server setup and cleanup:

```bash
./scripts/mcp-test-setup.sh
./scripts/mcp-test-cleanup.sh
```

## Backup Script

Database backup and restore:

```bash
./scripts/backup.sh
```

See [docs/content/backup.md](/docs/content/backup.md) for usage details.
