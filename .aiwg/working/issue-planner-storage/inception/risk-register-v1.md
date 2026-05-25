# Risk Register v1: Referenced Storage Mode + Scan-and-Ingest

**Issue**: fortemi/fortemi#736
**Phase**: Inception
**Date**: 2026-05-21
**Source**: @.aiwg/working/issue-planner-storage/synthesis.md §5

This register extends — does not replace — any existing `.aiwg/risks/` content. All risks carry an evidence tag (`established` = cross-stream consensus or documented incident; `emerging` = single-stream evidence; `speculative` = inferred from architectural reasoning) reflecting the hedging discipline carried forward from research streams.

## Severity × Likelihood Scale

Both axes 1-5. Score = Sev × Lik. Retirement column: Inception artifacts cannot retire technical risk; risks marked **E** must retire in Elaboration (PoCs, spikes, threat model), **C** retire during Construction (test pass), **O** are operationally accepted and tracked.

## Top 10 Risks

| ID | Risk | Cat | Sev | Lik | Score | Tag | WS | Mitigation | Owner-type | Retires |
|----|------|-----|-----|-----|-------|-----|----|-----------|-----------|---------|
| R-1 | Secret leakage from auto-indexing user-owned config files (.env, .pem, .ssh/, cloud creds) into pgvector embeddings | security | 5 | 4 | 20 | established | WS-3, WS-4 | Combined path-denylist + content-regex secret detection at ingest with hard-stop quarantine logging. Defaults per synthesis Decision 7 (PEM header regex, AWS access key pattern, GitHub PAT pattern, JWT pattern, path patterns for .env*, .pem, .key, id_rsa*, .ssh/, .gnupg/, .aws/credentials, .kube/config). Quarantine records visible via `GET /archives/{name}/quarantined-files`. | Security architect + Software implementer | E (threat model) + C (red-team test in WS-9) |
| R-2 | Multi-tenant boundary breach via crafted `source_path` (path traversal, symlink-out-of-root, sibling-tenant access) | security | 5 | 3 | 15 | established | WS-7, WS-9 | Path canonicalization + allowlist enforcement at archive-create time via env var `FORTEMI_REFERENCED_STORAGE_ROOTS`. Tenant-scoped root mandatory when `FORTEMI_MULTI_TENANT=true`. WS-9 test suite includes path-traversal cases (`../../../etc/passwd`), symlink-out-of-root, and cross-tenant access attempts. | Security architect | E (threat model + path-canonicalization PoC) + C (WS-9) |
| R-3 | Performance death on monorepo ingest — initial scan of a 100k-file repo runs out of memory or takes hours of GPU time | technical | 4 | 4 | 16 | emerging | WS-3, WS-4 | Default 10MB file-size cap; default ignore list excludes `node_modules/`, `vendor/`, `target/`, `dist/`, `.next/`, `__pycache__/`; threaded walker capped at `min(4, num_cpus)`; content-hash dedup prevents re-embedding identical files across rescans. Streaming `compute_content_hash_stream(path)` avoids loading large files into memory. | Architecture designer + Software implementer | E (performance spike on representative monorepo) + C |
| R-4 | Docker bind-mount FS events drop silently on `overlay2`, NFS, or Windows volumes — if live watching is ever added, index will silently drift | technical | 3 | 5 | 15 | established | WS-5 (deferred) | **Decision 4 retires this for v1 by deferring live watching entirely.** v1 ships with explicit `POST /rescan` only. Documented as a known limitation in WS-10 operator docs. When WS-5 is taken up, a separate design RFC is required before construction. | Architecture designer | Deferred — risk remains open for WS-5 |
| R-5 | Source directory disappears mid-query (NFS mount drops, USB disconnect) — all reads hard-fail, agent abandons conversation | operational | 3 | 3 | 9 | emerging | WS-7 | Decision 8: fail-open for reads (serve cached pgvector results with `stale: true` warning flag), fail-closed for writes/rescans (HTTP 503). Integrity sweep job detects offline state and emits metrics/log event for operator awareness. | Software implementer | C (failure-mode test) + O |
| R-6 | `CodeAstAdapter` regex extraction produces low-quality chunks for code search; semantic search precision suffers | technical | 3 | 4 | 12 | established | (out of scope for #736) | Tree-sitter upgrade is a parallel issue, explicitly out of scope per synthesis §7 non-goal #2. v1 ships with regex extraction: "acceptable, not great." Operators get a working feature; tree-sitter improves quality later without touching #736 architecture. | (deferred) | Deferred to separate epic |
| R-7 | Symlink loops or out-of-root symlinks cause runaway scan or unauthorized file access | security | 3 | 2 | 6 | emerging | WS-3 | `ignore` crate has built-in symlink-loop protection. Decision: never follow symlinks pointing outside the canonicalized root (configurable but default-off). Skip and log on out-of-root targets. WS-9 test suite covers symlink-out-of-root cases. | Software implementer | E (PoC) + C (WS-9) |
| R-8 | Derived artifact disk usage explodes for media-heavy Referenced archives (video transcripts, keyframes, 3D-model multi-view renders accumulate) | operational | 3 | 3 | 9 | speculative | WS-6, WS-10 | Document disk-usage model in operator docs (WS-10). Operator-tunable `FORTEMI_DERIVED_STORAGE_PATH` env var allows pointing derived artifacts to a separate volume. Per-archive disk-usage stats exposed via existing `manage_archives` MCP tool. | Software implementer + Technical writer | O (operational concern, documented) |
| R-9 | Rename detection failure surfaces as "file deleted + new file created" pairs in v1 — search results briefly show stale paths | technical | 2 | 4 | 8 | speculative | WS-4 (basic), WS-5 (improved) | v1: no special rename detection. Delete-event removes index entries; create-event adds new ones. v2 (with WS-5 live watching): content-hash correlation with 30s TTL pending-delete buffer per Stream A §6.2. User-visible impact in v1 is tolerable for on-demand-rescan model — paths refresh on next `POST /rescan`. | Software implementer | C (documented behavior) |
| R-10 | Unresolved deferred WS-5 (live watching) — operator may interpret synthesis Q-1 differently than recommended | scope | 2 | 3 | 6 | established | WS-5 (deferred) | **Known unresolved risk.** Synthesis Q-1 is the most important Phase 5 operator question: if the operator wants live watching at v1, the implementation cost is substantial (new long-running watcher process, lifecycle management, fallback polling scheduler, Docker bind-mount foot-gun). Compromise option: polling-only at 60s interval (no inotify). Decision documented and tracked; not retired in Inception. | Operator (Phase 5) + Architecture designer | Deferred — operator decision required at Phase 5 gate |

## Risk Categories Summary

- **Security (R-1, R-2, R-7)**: Highest-severity items; all retire via threat model in Elaboration and red-team tests in WS-9 during Construction
- **Technical (R-3, R-4, R-6, R-9)**: Performance and correctness; R-3 retires via performance spike in Elaboration; R-4, R-6 deferred by scope decision
- **Operational (R-5, R-8)**: Failure-mode and capacity concerns; retire via documentation and operational acceptance
- **Scope (R-10)**: Captures the deferred WS-5 decision as an open, tracked risk requiring operator input

## Known Unresolved (Carried Forward)

- **WS-5 (live filesystem watching)** — deferred to backlog; risk R-10 tracks the deferred state. v1 operator-facing semantics are "eventually consistent on operator action, not on filesystem events" (synthesis Decision 4). The risk remains live until either (a) WS-5 is constructed with its own design RFC, or (b) the operator accepts "on-demand rescan only" as the permanent model.
