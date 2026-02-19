# ADR-083: Brand Naming — Fortémi

**Status:** Accepted
**Date:** 2026-02-03
**Decision Makers:** roctinam

## Context

The project needed a permanent product name for public release. The internal name "matric-memory" was a development placeholder unsuitable for branding, domain registration, and package publishing.

## Decision

**Fortémi** (for-TAY-mee)

### Etymology
- **Italian/Musical** *forte* (forté): strength, strong point
- **Japanese** *美* (mi): harmony, beauty

Cross-language synthesis conveying: **strong harmony** — the balance of strength and elegance in knowledge management.

## Name Usage

| Context | Format |
|---------|--------|
| Product name | Fortémi |
| Repository | fortemi/fortemi |
| Rust crates | matric-core, matric-db, matric-api (internal, rename deferred) |
| npm packages | @fortemi/mcp, @fortemi/client |
| CLI command | fortemi |
| API paths | /api/v1/... (no name in path) |

## Clearance

All verified 2026-02-03:

- **Domains**: fortemi.com, fortemi.io, fortemi.info registered
- **Registries**: npm, crates.io, GitHub org, PyPI — all available
- **Trademarks**: No conflicts in USPTO, EUIPO, or web search

## Migration Plan

1. **Phase 1 (done)**: GitHub publication as `fortemi/fortemi`, CI publishes to `ghcr.io/fortemi/fortemi`
2. **Phase 2 (deferred)**: Internal crate rename matric-* → fortemi-*
3. **Phase 3 (deferred)**: Full brand launch with docs site and registry publication

## Consequences

- External-facing names use "Fortémi" / "fortemi"
- Internal Rust crate names remain `matric-*` until Phase 2
- Docker images tagged under `fortemi/fortemi`
- Gitea repo: `fortemi/fortemi` (migrated from `roctinam/matric-memory`)
