# Vision: Referenced Storage Mode + Scan-and-Ingest

**Issue**: fortemi/fortemi#736
**Phase**: Inception
**Date**: 2026-05-21
**Source**: @.aiwg/working/issue-planner-storage/synthesis.md

## One-Sentence Vision

Fortemi indexes user-owned source directories in place — owning the index, never the data — so a Fortemi-backed AI agent can search a local code tree with the same API surface used for managed archives, without copying a single source byte.

## Success Criteria (Measurable)

Per `.claude/rules/vague-discretion.md`: every criterion is checkable, not aspirational.

1. **Scan-and-ingest completes without OOM on a 10k-file fixture repo** running with default file-size cap (10MB) and default ignore list. Memory peak observable via `/usr/bin/time -v` or process metrics; "completes" = `archive_registry.scan_status` transitions `scanning → idle` with no errors logged.

2. **Existing search API returns results for ingested Referenced-archive chunks** within the archive's pgvector store. Verifiable: `POST /api/v1/search` with `X-Fortemi-Memory: <referenced-archive>` returns ≥1 result for a query whose terms appear in the source.

3. **No `write()` or `delete()` call ever lands on a `source_path`-rooted file during the lifetime of a Referenced archive**, including during `drop_archive_schema()`. Verifiable: WS-9 multi-tenant security test suite includes a drop-archive test that asserts source directory inode mtime unchanged and contents byte-identical post-drop.

4. **Path-canonicalization and allowlist enforcement reject `source_path` outside `FORTEMI_REFERENCED_STORAGE_ROOTS`** in multi-tenant deployments (`FORTEMI_MULTI_TENANT=true`). Verifiable: `POST /api/v1/archives/referenced` with `source_path` outside allowlist returns HTTP 400; WS-9 path-traversal test suite includes cases for `../`, symlink-out-of-root, and tenant-A-paths-from-tenant-B contexts.

5. **Pre-ingest secret-scan quarantines files matching the path-denylist OR content-regex denylist from synthesis Decision 7**, with skipped files visible via `GET /api/v1/archives/{name}/quarantined-files`. Verifiable: dropping a fixture file containing a PEM `-----BEGIN RSA PRIVATE KEY-----` header into a scanned directory produces a quarantine record and zero embeddings for that file.

6. **Backward compat: all existing archives migrate cleanly with `storage_mode='managed'` default**, and every current API handler / MCP tool invocation that does not pass `storage_mode` continues to behave identically to pre-migration behavior. Verifiable: full existing test suite (`cargo test --workspace`) passes on the migration branch.

## Anti-Vision (What Success Does NOT Include)

To prevent scope creep at Phase 5 (per synthesis §7 and `scoped-reasoning` rule):

- **Real-time filesystem watching.** Success does NOT mean "edit a file in your editor and the index updates within seconds." Live watching is deferred to WS-5. v1 semantics are eventual-consistency-on-operator-action via `POST /rescan`.
- **Tree-sitter-quality chunking.** v1 ships with the existing `CodeAstAdapter` regex extraction. "Better than nothing" — not "as good as it could be." Tree-sitter is a separate parallel improvement.
- **Remote storage.** Success does NOT mean "point Fortemi at an S3 bucket." Storage is local filesystem only (`PathBuf`-addressed). Remote is a separate larger epic.
- **Zero-config secret protection.** Success does NOT mean "every secret in every conceivable format is caught." The default denylist catches the 95% case (PEM, AWS, GitHub PAT, JWT, common path patterns). Operators with custom secret formats need their own denylist contributions.
- **Web UI for archive management.** Success does NOT include a graphical archive-create flow. CLI and API are the v1 surface.
- **Source-path migration.** Success does NOT mean an archive can change its `source_path` after creation. Drop-and-recreate is the supported workflow.
- **Cross-archive overlap enforcement.** Success does NOT mean preventing two archives from referencing overlapping directories. Overlap is allowed with a warning (synthesis Q-6).
