# Contributing to Fortémi

Thank you for your interest in contributing to Fortémi! This guide covers development workflows, testing practices, and contribution guidelines.

## Getting Started

### Prerequisites
- Rust 1.70+
- PostgreSQL 16+ with pgvector extension
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

# Run tests
cargo test --workspace
```

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
3. Run tests and linters
4. Push and open a PR
5. Address review feedback

Follow project conventions for PR descriptions and commit messages.

## Testing

### Unit Tests
```bash
cargo test                  # All tests in current crate
cargo test --workspace      # All crates
cargo test test_name        # Specific test
```

### Integration Tests
```bash
# API integration tests
cargo test -p matric-api --test '*'

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
1. Run tests: `cargo test --workspace`
2. Run linters: `cargo clippy -- -D warnings`
3. Update versions in `Cargo.toml` and `mcp-server/package.json`
4. Update `CHANGELOG.md` with release notes
5. Commit: `git commit -m "chore: release vYYYY.M.PATCH"`
6. Tag: `git tag -a vYYYY.M.PATCH -m "vYYYY.M.PATCH - Release title"`
7. Push: `git push origin main --tags`
8. Create GitHub release with highlights

See `docs/content/releasing.md` for full details.

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
