# ADR-015: Cargo Workspace with Domain-Driven Crates

**Status:** Accepted
**Date:** 2026-01-02
**Deciders:** roctinam

## Context

matric-memory is a multi-component system with:
- HTTP API layer
- Database access layer
- Search functionality (FTS + semantic)
- AI inference backends
- Background job processing
- Cryptographic operations

A monolithic crate would result in:
- Long compile times
- Tangled dependencies
- Difficult testing in isolation
- Feature flag complexity

## Decision

Use a Cargo workspace with domain-driven crate separation:

```
crates/
├── matric-core      # Shared types, traits, models
├── matric-db        # PostgreSQL repositories
├── matric-search    # Hybrid search (FTS + semantic + RRF)
├── matric-inference # AI backends (Ollama, OpenAI)
├── matric-crypto    # Encryption, key derivation
├── matric-jobs      # Background job worker
└── matric-api       # Axum HTTP server
```

## Consequences

### Positive
- (+) Parallel compilation of independent crates
- (+) Clear dependency graph (no circular deps)
- (+) Isolated testing per domain
- (+) Feature flags scoped to relevant crates
- (+) Reusable components (matric-core across all)
- (+) Incremental compilation benefits

### Negative
- (-) More files to manage (7 Cargo.toml files)
- (-) Version coordination across crates
- (-) Import paths longer (matric_core::models::Note)
- (-) Initial setup complexity

## Implementation

**Code Location:** `Cargo.toml` (workspace root)

**Workspace Configuration:**

```toml
[workspace]
resolver = "2"
members = [
    "crates/matric-core",
    "crates/matric-db",
    "crates/matric-search",
    "crates/matric-inference",
    "crates/matric-crypto",
    "crates/matric-jobs",
    "crates/matric-api",
]

[workspace.package]
version = "2026.1.0"
edition = "2021"
license = "BSL-1.1"

[workspace.dependencies]
# Internal crates
matric-core = { path = "crates/matric-core" }
matric-db = { path = "crates/matric-db" }
# ... etc
```

**Dependency Graph:**

```
                    matric-api
                        │
           ┌────────────┼────────────┐
           ▼            ▼            ▼
      matric-jobs  matric-search  matric-inference
           │            │            │
           └────────────┼────────────┘
                        ▼
                   matric-db ◄─── matric-crypto
                        │
                        ▼
                   matric-core
```

**Crate Responsibilities:**

| Crate | Purpose | Key Dependencies |
|-------|---------|------------------|
| matric-core | Types, traits, models | serde, uuid, chrono |
| matric-db | PostgreSQL repos | sqlx, pgvector, matric-core |
| matric-search | Hybrid search, RRF | matric-core, matric-db |
| matric-inference | AI backends | reqwest, async-trait, matric-core |
| matric-crypto | Encryption | aes-gcm, argon2, matric-core |
| matric-jobs | Job worker | matric-db, matric-inference |
| matric-api | HTTP API | axum, matric-* |

**Compilation Order:**

1. matric-core (no deps)
2. matric-db, matric-inference, matric-crypto (parallel)
3. matric-search, matric-jobs (depends on db)
4. matric-api (depends on all)

**Testing Strategy:**

```bash
# Test single crate
cargo test -p matric-search

# Test all crates
cargo test --workspace

# Test with features
cargo test -p matric-inference --features openai
```

## Version Strategy

Using CalVer `YYYY.M.PATCH` (e.g., `2026.1.0`) across all crates:
- All crates share workspace version
- Single version bump for releases
- Git tag: `v2026.1.0`

## References

- Cargo Workspaces: https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html
- Domain-Driven Design principles for module boundaries
