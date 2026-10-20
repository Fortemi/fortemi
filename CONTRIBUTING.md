# Contributing to Fortémi

Thank you for your interest in contributing to Fortémi! This guide covers development workflows, testing practices, and contribution guidelines.

## Getting Started

### Prerequisites
- Rust 1.70+
- PostgreSQL **18 or later** with pgvector extension — required, not optional. matric-api uses RFC 9562 `uuidv7()` and `uuid_extract_timestamp()` built-ins that ship in PG 18. Older versions are rejected at startup with a clear error pointing at PGDG.
- Node.js 18+ (for MCP server)

### Setup
```bash
# Clone the repository
git clone https://github.com/fortemi/fortemi.git
cd fortemi

# Install git hooks
./scripts/install-hooks.sh

# Set up database
psql -U postgres -c "CREATE DATABASE matric;"
psql -U postgres -d matric -c "CREATE EXTENSION vector;"

# Run migrations
cargo install sqlx-cli
sqlx migrate run

# Build the affected crate (see change-scoped verification below)
cargo build --locked -p matric-api
```

API integration tests require `DATABASE_URL` to explicitly select a disposable,
fully migrated database. They create and remove schemas and fixtures; do not use a
personal or shared application database. The hosted non-owner isolation matrix
also requires `FORTEMI_HOSTED_CREATE_TEST_ADMIN_URL` pointing at that disposable
database with permission to create its temporary runtime role.

### sqlx compile-time query checks

Fortemi uses `sqlx::query!` macros that verify SQL against a real Postgres at **compile time**. The first build on a new machine fails with a confusing "missing graph" error if neither of these is true:

- `DATABASE_URL` is set and points at a Postgres that already has all migrations applied, **or**
- `.sqlx/` is populated with offline query metadata and `SQLX_OFFLINE=true` is set

Pick one of:

**A) Live database (recommended for active development):**

```bash
export DATABASE_URL="postgres://matric:matric@localhost:5432/matric"
sqlx migrate run        # apply migrations
cargo build             # query!() macros check against the live DB
```

**B) Offline metadata (recommended for CI / users who just want to compile):**

```bash
# One-time, against a Postgres with all migrations applied:
cargo install sqlx-cli
DATABASE_URL=postgres://matric:matric@localhost:5432/matric \
    cargo sqlx prepare --workspace
git add .sqlx/
git commit -m "chore: refresh sqlx offline metadata"

# Subsequent builds compile without a database:
SQLX_OFFLINE=true cargo build
```

Regenerate `.sqlx/` whenever a migration or `query!()` call changes. CI runs `cargo sqlx prepare --check --workspace` to flag drift.

### Dependency checks (workspace-wide)

`scripts/check-deps.sh` runs the same dependency-health checks CI runs — across both the Rust workspace and `mcp-server/` in one command:

```bash
./scripts/check-deps.sh           # full: cargo check + cargo audit + cargo deny + npm ci + npm audit
./scripts/check-deps.sh --fast    # skip cargo audit/deny (avoids advisory-DB git fetch)
```

Run this before pushing PRs that touch `Cargo.toml`, `Cargo.lock`, `mcp-server/package.json`, or `mcp-server/package-lock.json`. CI enforces:

- `cargo audit` — RustSec advisories (allowlist in `.cargo/audit.toml`)
- `cargo deny check` — advisories + licenses + duplicate versions + sources (policy in `deny.toml`)
- `npm ci` in `mcp-server/` — lockfile sync verification
- `npm audit --audit-level=high` in `mcp-server/` — npm advisories

Required tooling: `cargo install --locked cargo-audit cargo-deny` (one-time).

## Development Workflow

### Git Hooks
Pre-commit hooks ensure code quality before commits:
- `cargo fmt --check` - Verify code formatting
- `cargo clippy -- -D warnings` - Check for lint issues

If checks fail:
```bash
cargo fmt --all                    # Fix formatting
cargo clippy --fix --all-targets   # Auto-fix clippy issues
```

To bypass hooks (not recommended): `git commit --no-verify`

### Commit Messages
Follow conventional commits format:
```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`, `perf`

Examples:
- `feat(search): add trigram emoji search support`
- `fix(api): handle null values in tag export`
- `docs(adr): add ADR-029 for shard versioning`

### Pull Requests
1. Create a feature branch: `git checkout -b feat/your-feature`
2. Make changes with clear commits
3. Build and run change-scoped tests and linters; record commands and counts
4. Push and open a PR
5. Address review feedback, merge, and monitor main CI plus its comprehensive test handoff

Follow project conventions for PR descriptions and commit messages.

## Testing

### Change-scoped verification

Pull requests do not run CI. Before opening or merging a PR, build locally
and run the tests for the touched crates/files and their direct neighbours
(callers, consumers, and shared fixtures). Select affected packages with `-p`
and test targets or filters as appropriate; a full local workspace test run is
not a pre-PR requirement. Include the CI-equivalent features and environment
for each selected test, as defined in `.gitea/workflows/ci-builder.yaml` and
`.gitea/workflows/test.yml`.

For example, for a hosted API/SSE change with crypto neighbours:

```bash
# Use a disposable PostgreSQL 18 database with CI extensions and migrations.
export DATABASE_URL="<DATABASE_URL>"
export REDIS_URL="redis://localhost:6379/0"
cargo build --locked -p matric-api --features hosted-auth,kms-vault
cargo test --locked -p matric-api --features hosted-auth,kms-vault --bin matric-api sse_tests -- --test-threads=1
cargo test --locked -p matric-crypto --features kms-vault --lib
```

Expand that selection to all affected tests and direct neighbours; these commands
are an example, not a universal minimum. Hosted create/bootstrap tests also need
`FORTEMI_HOSTED_CREATE_TEST_ADMIN_URL` or
`FORTEMI_HOSTED_BOOTSTRAP_TEST_ADMIN_URL` for the same disposable database.
Use `--features migrations` for DB migration tests, keep shared-database tests
serial (`-- --test-threads=1`), and run worker integration tests separately before
other database tests, as CI does. Do not skip required infrastructure tests.
For workflow/Python changes, run `python3 -m unittest discover -s tests --pattern='test_*.py'`
and the relevant workflow policy verifiers; for docsite changes, run
`npm ci`, `npm run docs:build`, and `npm run docs:check-assets`.

Record the exact commands, features/environment, selected scope and neighbour
rationale, and passed/failed/ignored counts in the PR's **Verification** section.
Record build results and any unavailable checks explicitly.

CI runs after merge on the push to `main`: ci-builder hands off to comprehensive
`test.yml`. `delivery.require_ci_green` applies to that post-merge main run and
its test handoff. Record their run IDs and wait for both to pass before declaring
work complete. Fix a red main forward immediately on a new branch: reproduce,
fix, verify locally, merge, and monitor the next main run.


### Unit Tests
```bash
cargo test                  # All tests in current crate
cargo test --workspace      # Optional comprehensive local run
cargo test test_name        # Specific test
```

### Integration Tests
```bash
# API integration tests
cargo test -p matric-api --features hosted-auth,kms-vault --test '*'

# Search integration tests
cargo test -p matric-search --test '*'
```

### Test Organization
- Unit tests: In `mod tests` at bottom of source files
- Integration tests: In `tests/` directory
- Fixtures: In `tests/fixtures/`
- Test helpers: In `tests/helpers/`

## Shard Schema Changes

When modifying the shard format (ShardManifest, note records, etc.):

### Version Bump
Update `CURRENT_SHARD_VERSION` in `crates/matric-core/src/shard/version.rs`:
- **MAJOR**: Breaking changes that require migration
- **MINOR**: New optional fields (backward compatible)
- **PATCH**: Bug fixes only

### Migration Handler
For MAJOR changes, create a migration handler implementing `ShardMigration` trait in `crates/matric-core/src/shard/migrations/`.

Example structure:
```rust
// crates/matric-core/src/shard/migrations/v1_0_to_v2_0.rs
use super::ShardMigration;

pub struct V1ToV2Migration;

impl ShardMigration for V1ToV2Migration {
    fn from_version(&self) -> Version {
        Version::new(1, 0, 0)
    }

    fn to_version(&self) -> Version {
        Version::new(2, 0, 0)
    }

    fn migrate(&self, data: ShardData) -> Result<ShardData> {
        // Transform data from v1 to v2
        Ok(data)
    }
}
```

### Reserved Fields
When removing or renaming fields, add old names to `crates/matric-core/src/shard/reserved.rs` to prevent future reuse.

### Testing Requirements
- Add tests for new/changed fields
- Test full import/export cycle
- Test migration from previous version
- Test both new and legacy formats

### Pull Request Template
Use the `shard_schema_change.md` PR template for schema changes.

See ADR-029 for full versioning specification.

## Documentation

### Code Documentation
- Add doc comments to public APIs
- Include examples for complex functions
- Document error conditions and edge cases

### Architecture Decision Records (ADRs)
Document significant architectural decisions in `docs/adr/`.

Format:
```markdown
# ADR-XXX: Title

**Status:** Accepted | Proposed | Deprecated
**Date:** YYYY-MM-DD
**Deciders:** Names

## Context
What problem are we solving?

## Decision
What did we decide?

## Consequences
What are the implications?
```

### User Documentation
- Update relevant docs in `docs/content/`
- Add usage examples for new features
- Document breaking changes in CHANGELOG.md

## Release Process

Fortémi uses CalVer versioning: `YYYY.M.PATCH`

### Release Checklist
1. Verify changes locally using the change-scoped rule above.
2. Align release versions and update the changelog and announcement.
3. Merge the candidate to main and require green post-merge CI.
4. Dispatch ci-builder on `main` with the exact candidate `source_sha`, and
   dispatch `docsite-build`. Require those runs and the comprehensive `test.yml`
   handoff to be green; record the run IDs in the release PR/notes.
5. From a clean checkout at that validated current main commit, use
   `tools/release/cut-tag.sh`, push the signed tag, and verify release publication.

See [the release process](docs/content/releasing.md) for the exact dispatch and
signing procedure. Do not cut a tag before the release validation run passes.

## Code Style

### Rust
- Follow Rust API Guidelines
- Use `cargo fmt` for formatting
- Fix all `cargo clippy` warnings
- Prefer explicit error handling over panics
- Use type aliases for complex types
- Add `#[must_use]` for important return values

### SQL
- Use parameterized queries (sqlx)
- Add migrations for schema changes
- Include rollback migrations when possible
- Test migrations on realistic data

### TypeScript (MCP Server)
- Use TypeScript strict mode
- Add JSDoc comments for public functions
- Use async/await consistently
- Handle errors explicitly

## Getting Help

- GitHub Issues: Report bugs and request features
- Discussions: Ask questions and share ideas
- Documentation: Check `docs/` for guides

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.
