# Test Strategy: Referenced Storage Mode + Scan-and-Ingest

**Issue**: fortemi/fortemi#736
**Phase**: Phase 3 — SDLC Corpus Generation (issue-planner workflow)
**Date**: 2026-05-21
**Sole input**: `@.aiwg/working/issue-planner-storage/synthesis.md`

This is the master test strategy for the Referenced storage mode epic. It defines the test pyramid, categorizes test types per workstream, names the fixture corpus, and wires the suite into Fortemi's existing CI on the `matric-builder` runner. Test cases themselves are deferred to `test-plan-construction.md` (per-workstream) and `tenant-isolation-regression-suite.md` (TI-EXTSTORAGE-* security suite).

---

## 1. Scope

### In Scope

- Unit tests for `ReferencedBackend: StorageBackend` (WS-1), `ScanWalker` (WS-3), secret detection (WS-3), `DirectoryScanHandler` (WS-4), companion-derived-location dispatch (WS-6)
- Integration tests for schema migration (WS-2), API surface (WS-7), MCP tool extensions (WS-8), end-to-end scan-and-ingest
- E2E tests for the operator-visible workflow: create Referenced archive → scan → search → quarantine inspection
- Security regression suite (TI-EXTSTORAGE-1 through TI-EXTSTORAGE-10) covering cross-tenant isolation, path traversal, symlink escape, secret quarantine, mount-disappearance failure modes
- Performance smoke benchmark (informational, non-gating): 10k-file scan completion time and chunk-throughput rate
- Test data fixtures under `test-data/external-storage/`

### Out of Scope

- Live filesystem watching tests (WS-5 is deferred per Decision 4)
- Cross-archive overlap conflict tests (Decision Q-6 resolves to allow-with-warning; no enforcement, no test)
- Remote storage backend tests (explicit non-goal §7.1)
- Tree-sitter parsing quality regression (out of scope per §7.2; regex extraction is the v1 baseline)
- Tests that depend on Docker bind-mount inotify behavior (deferred with WS-5)

---

## 2. Test Pyramid for This Epic

```
                  /\
                 /  \      e2e (5-10 scenarios, full pipeline)
                /----\
               /      \    security regression (TI-EXTSTORAGE-1..10)
              /--------\
             /          \  integration (DB + filesystem + API ~40-60 cases)
            /------------\
           /              \ unit (Rust trait/impl, walker, secret detect ~80-120 cases)
          /----------------\
```

Numerical targets are floors, not ceilings. The construction phase should bias toward more unit tests where possible (cheaper, faster, more focused) and add integration tests where unit isolation isn't achievable (e.g., anything touching `#[sqlx::test]`-incompatible operations like enum value additions, per CLAUDE.md PostgreSQL Migration Compatibility).

### Layer Definitions

| Layer | What it tests | Test framework | Speed budget per test |
|---|---|---|---|
| Unit | Single function/trait impl in isolation; mocked dependencies | `#[test]` and `#[tokio::test]` in-crate | <50ms |
| Integration (transactional) | Anything that fits in a PG transaction (most DB tests) | `#[sqlx::test]` (transaction rollback) | <500ms |
| Integration (non-transactional) | Migrations, enum value additions, schema clone, anything with `CREATE INDEX CONCURRENTLY` | `#[tokio::test]` with manual pool setup (per CLAUDE.md) | <2s |
| E2E | Full ingest scenario via API, asserting search results | `#[tokio::test]` with full bundle running locally OR via `cargo test --test e2e` | <30s |
| Security regression | TI-EXTSTORAGE-* suite — explicit attacker scenarios | `#[tokio::test]` with manual pool, often spans multiple tenant schemas | <5s |
| Performance smoke | 10k-file scan benchmark | `cargo bench` (informational, runs in nightly CI) | n/a (records elapsed time) |

---

## 3. Test Categories with Examples

These are illustrative test names per category — not exhaustive. The actual test inventory is in `test-plan-construction.md`.

### 3.1 Unit (WS-1, WS-3 primary)

- `referenced_backend_read_returns_correct_bytes` — `ReferencedBackend::read(absolute_path)` returns the file contents byte-for-byte (synthesis §3 Decision 2)
- `referenced_backend_write_returns_not_supported_error` — confirms `write()` is a no-op-returning-error and never modifies user files (§3 Decision 2, §2.1 consensus)
- `referenced_backend_delete_returns_not_supported_error` — same for `delete()` (defense-in-depth, §2.1 finding 2)
- `referenced_backend_resolve_path_returns_literal_path` — `resolve_path()` returns the absolute path as recorded, no transformation
- `scan_walker_respects_gitignore` — fixture directory with `.gitignore` correctly excludes listed paths (§3 Decision 7)
- `scan_walker_skips_default_ignored_dirs` — `node_modules/`, `target/`, `.git/`, etc., excluded (§3 Decision 7 ignore list)
- `secret_detector_catches_pem_private_key_header` — file containing `-----BEGIN RSA PRIVATE KEY-----` is quarantined (§3 Decision 7 content-based denylist)
- `secret_detector_catches_aws_access_key_pattern` — file containing `AKIA[A-Z0-9]{16}` pattern is quarantined
- `secret_detector_catches_github_pat_pattern` — file containing `ghp_[a-zA-Z0-9]{36}` is quarantined
- `secret_detector_catches_jwt_pattern` — file containing JWT-shaped string is quarantined (with confidence threshold per §3 Decision 7)
- `path_denylist_catches_dotenv` — `.env`, `.envrc`, `.env.local` all flagged before content scan
- `path_denylist_catches_ssh_keys` — `id_rsa`, `id_ed25519`, `.ssh/` directory all flagged
- `streaming_blake3_hash_matches_full_read_hash` — `compute_content_hash_stream(path)` produces same hash as `compute_content_hash(bytes)` for files >100MB (synthesis §3 Decision 2 streaming requirement)

### 3.2 Integration — Filesystem + Database (WS-2, WS-4, WS-6)

- `migration_adds_storage_mode_column_to_archive_registry` — applies the new migration, asserts schema shape
- `migration_preserves_existing_archives_as_managed` — pre-existing archives default to `storage_mode='managed'` after migration (backward-compat gate, §3 Decision 1)
- `create_referenced_archive_with_valid_path` — `ArchiveRepository::create_referenced_archive(name, source_path)` records the archive correctly
- `create_referenced_archive_rejects_nonexistent_path` — returns error at creation time, before any scan job is queued
- `scan_and_ingest_small_fixture_repo_completes` — full pipeline: walk → hash → dedup → insert → embed → search returns results
- `scan_idempotent_on_unchanged_files` — running scan twice produces zero new blobs/chunks (BLAKE3 dedup, §2.1 finding 4)
- `scan_re_ingests_on_content_change` — file modified between scans produces new chunks; old chunks removed (re-ingest, not patch — §3 Decision 5)
- `derived_artifact_routes_to_companion_location` — ingesting a fixture image in a Referenced archive places thumbnail in `{FILE_STORAGE_PATH}/derived/{archive_id}/`, not in source dir (§3 Decision 3)
- `drop_referenced_archive_preserves_source_path` — `drop_archive_schema()` for a Referenced archive removes the PG schema and the companion derived location but does not touch `source_path` (synthesis §2.3 constraint 6)
- `quarantine_log_records_skipped_secrets` — file with PEM header is logged in quarantine table with reason "pem_private_key_header_match"
- `quarantine_log_visible_via_repository_query` — `ArchiveRepository::list_quarantined_files(archive_name)` returns the quarantined entries

### 3.3 E2E — Full Pipeline (WS-4, WS-7)

- `e2e_create_referenced_archive_via_api_and_search` — `POST /api/v1/archives/referenced` → wait for scan completion → `POST /api/v1/search` returns indexed content (the headline scenario from #736)
- `e2e_rescan_endpoint_picks_up_new_files` — add a file to the source dir, call `POST /api/v1/archives/{name}/rescan`, poll `GET .../scan-status`, confirm new file appears in search results
- `e2e_quarantine_workflow_end_to_end` — source dir contains an `id_rsa` file; after scan, `GET /api/v1/archives/{name}/quarantined-files` returns the file with reason; chunks/embeddings for that file do not exist
- `e2e_referenced_archive_read_only_via_api` — `POST` to notes/attachments under a Referenced archive returns 403 with clear error message (§3 Decision 2 + WS-7 gate)
- `e2e_search_returns_results_with_correct_archive_attribution` — search results from a Referenced archive carry the archive name and storage_mode in response metadata

### 3.4 Security Regression — TI-EXTSTORAGE Suite (WS-9)

Full suite definition in `tenant-isolation-regression-suite.md`. Summary:

- TI-EXTSTORAGE-1 through TI-EXTSTORAGE-10 — cross-tenant isolation, path traversal, symlink escape, secret quarantine, mount-disappearance failure modes, cross-tenant dedup independence, archive-drop source preservation, MCP cross-session protection

### 3.5 Performance Smoke (Informational)

- `bench_scan_10k_files_with_embedding` — fixture directory of 10,000 small text files; measures wall-clock time from `POST /archives/referenced` to `scan_status='idle'`. Per synthesis §6 Q-7 recommendation (B): expected <10 minutes on default CPU embedding. Records but does not gate.
- `bench_scan_walker_throughput` — files-walked-per-second across the small/medium/large fixture sizes. Records baseline; alert in nightly CI if regresses >20%.

---

## 4. Test Data Strategy

All fixtures live under `test-data/external-storage/` in the repo (not gitignored, not under `.aiwg/`). Each fixture is a directory structure intended to be referenced as a Referenced archive `source_path`. Fixtures are checked into git so the test suite is reproducible across CI runs and operator workstations.

### 4.1 Required Fixtures

| Fixture | Purpose | Contents |
|---|---|---|
| `small-repo/` | Happy-path baseline; <100 files, ~1MB total; mixed text + code | README.md, src/{a.rs, b.rs, c.ts}, docs/*.md, package.json, .gitignore |
| `with-secrets/` | Secret detection regression | Files matching every pattern in §3 Decision 7 — id_rsa, .env, AWS key, GitHub PAT, JWT, PEM PRIVATE KEY headers in unexpected file names |
| `with-symlink-escape/` | Symlink-out-of-root protection | Source dir contains a symlink pointing to `/etc/passwd` (or platform equivalent — for test portability, target a tmpdir file outside the fixture) |
| `with-symlink-loop/` | Symlink loop protection | Source dir contains symlink cycle `a/ → b/ → a/` |
| `with-large-files/` | Streaming hash + file-size cap | Contains files >10MB (above default cap) and files at exactly 10MB boundary |
| `with-non-utf8-paths/` | Path-encoding edge cases | Filenames with non-UTF-8 bytes (where filesystem permits), CJK characters, emoji |
| `with-gitignore/` | `.gitignore` honored | `.gitignore` excludes `target/` and `*.log`; fixture contains both; scan must skip both |
| `with-permission-denied/` | Unreadable subdir handling | Subdirectory with mode 0 (no permissions); scan should log warning, not fail (§3 WS-3 gate) |
| `with-many-small-files/` | Performance smoke | 10,000 small files (~100 bytes each) for `bench_scan_10k_files_with_embedding` |
| `with-overlapping-paths/` | Q-6 overlap allow-with-warning | Two fixtures share a parent path; test that creating both archives produces warning but succeeds |
| `with-mixed-media/` | Derived-artifact routing | Source dir contains 1 image, 1 audio, 1 text; tests that thumbnails/transcripts land in companion dir (WS-6) |
| `empty-dir/` | Edge case | Empty directory; scan should complete with zero blobs created, status `idle` |
| `single-file-only/` | Edge case | One file; tests that walker handles minimal case |

### 4.2 Fixture Generation Strategy

Fixtures are static where possible. For the 10k-file fixture, a generator script `test-data/external-storage/with-many-small-files/generate.sh` (idempotent) creates the files; it runs once locally and the resulting files are committed (size impact: ~1MB total, acceptable). For non-UTF-8 paths, a Rust-side test helper creates them at runtime if the filesystem allows (some CI filesystems may reject; gate the test on filesystem capability detection).

### 4.3 Sensitive-Data Hygiene

The `with-secrets/` fixture contains DELIBERATELY FAKE secrets — patterns that match the regexes but are not valid credentials. Example: `AKIAIOSFODNN7EXAMPLE` is the AWS-documented example access key. Document this in `test-data/external-storage/with-secrets/README.md` to prevent the fixture itself from being flagged by secret scanners running on the Fortemi repo. Per CLAUDE.md token-security rule, the fixture README explicitly states "these are not real credentials; do not rotate."

---

## 5. CI Integration

### 5.1 Existing CI Surface (per CLAUDE.md)

- `ci-builder.yaml` — main CI pipeline (build, test, deploy on push to main)
- `test.yml` — unit & integration tests with coverage on push and PRs
- Runner: `matric-builder` with Docker (PostgreSQL 18 + pgvector + PostGIS containers), Rust toolchain, `cargo-llvm-cov`

### 5.2 Required CI Changes for This Epic

| Change | Where | Gate |
|---|---|---|
| Add `test-data/external-storage/` to checkout (already covered by default — full repo clone) | (none) | none |
| Run new test fixtures' generator script before test suite | `test.yml` pre-test step | Must succeed; fail CI if fixtures can't be created |
| Add new integration test crate or test target if needed | `Cargo.toml` workspace | Tests must compile |
| Expose `FILE_STORAGE_DERIVED_PATH` test env var pointing to a tmpdir | `test.yml` env block | Per-test isolation via unique subdir |
| Allowlist test fixtures for `FORTEMI_REFERENCED_STORAGE_ROOTS` env var in test runs | `test.yml` env block | Tests can create Referenced archives without hitting the allowlist gate (synthesis Q-5) |
| Run security regression suite (TI-EXTSTORAGE) as a separate test target, fail-fast on any TI-* failure | `test.yml` matrix job | All TI-EXTSTORAGE-* tests must pass on every PR |
| Add nightly performance smoke run (informational, non-gating) | new `nightly-benchmarks.yml` workflow | Records baseline; emits warning if 20%+ regression |

### 5.3 No Changes to `matric-builder` Image

The new fixtures are pure filesystem content (no new system packages required). The runner already has Rust, PostgreSQL, and the standard cargo toolchain — sufficient for the entire new test surface.

### 5.4 Coverage Reporting

`cargo-llvm-cov` already runs in `test.yml`. New code in `matric-jobs/src/scan_walker.rs`, `matric-jobs/src/directory_scan_handler.rs`, and `matric-db/src/file_storage.rs` (Referenced backend additions) will be reported. Target coverage in §6 below.

---

## 6. Test Coverage Targets

Per-workstream coverage floors for **new code introduced by this epic**. Existing code coverage is not the target — only new code under `WS-N` is held to these floors.

| Workstream | Coverage Floor | Rationale |
|---|---|---|
| WS-1 (Storage Backend Abstraction) | 90% | Trait surface is small and critical; defense-in-depth depends on `write`/`delete` no-op verification |
| WS-2 (Schema and Registry) | 85% | Migration logic + repository methods; harder to unit-test, so floor slightly lower |
| WS-3 (Walker + Ignore + Secret Scan) | 90% | Security-critical (secret detection); every regex path needs a test |
| WS-4 (Scan-and-Ingest Pipeline) | 80% | Orchestration layer; integration tests do most of the work |
| WS-6 (Derived Artifact Companion Location) | 85% | Small dispatch surface; high importance for source-preservation invariant |
| WS-7 (API Surface) | 85% | New routes + middleware gating; e2e tests carry significant coverage |
| WS-8 (MCP Tool Surface) | 80% | Node.js codebase; coverage measured separately via Node test tooling |
| WS-9 (Security Tests) | 100% of TI-EXTSTORAGE-* | Every TI-* scenario must have an automated test; security audit cannot accept "untested" boxes |
| WS-10 (Documentation) | n/a | Coverage doesn't apply; doc-sync skill verifies link integrity |

**Aggregate floor**: 85% of new code lines covered by automated tests. Coverage below this floor blocks PR merge for this epic.

---

## 7. Mutation Testing

**Status**: Optional for v1, **flagged for follow-up**.

Mutation testing (via `cargo-mutants` or `mutest`) would be high-value for the secret-detection module (WS-3) — a mutation that flips a regex pattern's match logic should cause at least one test to fail. If those mutations survive, the test suite has gaps.

**Recommendation**: Run `cargo-mutants` against `crates/matric-jobs/src/scan_walker.rs` and `crates/matric-jobs/src/directory_scan_handler.rs` as a one-time exercise during construction phase, document findings in `.aiwg/quality/mutation-testing-baseline.md`, and target ≤10 surviving mutations in the secret-detection module. Do NOT gate CI on this — it's a quality probe, not a release gate.

**Why not gating**: Mutation testing is slow (often 10-100x normal test runtime) and Fortemi's existing CI does not run it. Adding it as a gate would create a new operational burden disproportionate to the v1 risk.

---

## 8. References

- @.aiwg/working/issue-planner-storage/synthesis.md — sole Phase 2 input
- @.aiwg/working/issue-planner-storage/testing/tenant-isolation-regression-suite.md — TI-EXTSTORAGE-1..10 detail
- @.aiwg/working/issue-planner-storage/testing/test-plan-construction.md — per-workstream test inventory
- @.aiwg/working/issue-planner-storage/research-vendor-docs.md — Stream C source survey (extraction_handler, file_storage trait, archive_registry)
- @CLAUDE.md (Fortemi root) — Testing Standards, PostgreSQL Migration Compatibility, CI Runner notes
- @.claude/rules/anti-laziness.md — Rule 8 (never suppress CI signals), Test Analysis section
- @.claude/rules/vague-discretion.md — measurable completion criteria
- @.claude/rules/executable-feedback.md — coverage requirements per code type
