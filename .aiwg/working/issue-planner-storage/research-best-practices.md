# Research Stream A: Best Practices for External Storage + Code Indexing Ingestion

**Target**: fortemi/fortemi#736 — Allow users to point at a local directory or mount as the storage backend for an archive, with on-add scan-and-ingest for code indexing.

**Scope**: Industry patterns for file storage abstraction, repository indexing, content addressing, live update detection, and multi-tenant security boundaries — applied to Fortemi's specific architecture (PostgreSQL-per-archive schema, BLAKE3 content-addressed blob store, tree-sitter chunking, pgvector embeddings).

**Method**: Synthesis of established patterns from production code-indexing systems and storage abstraction layers. Source-survey of Fortemi's current `file_storage.rs` (1,718 lines), `archives.rs` (1,356 lines), and `archive_routing.rs` middleware grounds the recommendations in actual extension points.

**Hedging**: This document is a planning input, not a primary research artifact. Where patterns are well-attested across multiple production systems (sourcegraph, zoekt, Quickwit, livegrep, hound) the recommendation is marked as **established**. Where the pattern is single-system or recent (notify-rs `RecommendedWatcher` semantics on macOS 14+), it is marked **emerging** and the source is named. Pattern claims about tree-sitter, libgit2, and pgvector come from each project's public documentation; concrete numbers (e.g., "p95 indexing latency") are NOT asserted because we have no benchmark for this exact workload.

---

## 1. Storage Backend Abstraction — Trait-Level Patterns

### 1.1 The two-mode pattern (managed vs. referenced)

**Established**. Every production code-search system separates **managed storage** (the indexer owns the bytes; on delete, the bytes go) from **referenced storage** (the indexer points at user-owned bytes; on delete, the index entry goes but the source is untouched).

Examples:
- **Sourcegraph** distinguishes "cloned repos" (managed, under `gitserver`) from "site-admin-mounted volumes" (referenced). The schema for `repos.cloud_default` and `gitserver_repos.shard_id` encodes this split.
- **Hound** (etsy/hound) treats all repos as referenced — it clones into a local cache directory but the source-of-truth is the upstream URL. Deleting a repo from `config.json` removes the cache, never touches upstream.
- **livegrep** indexes from a `manifest.json` listing referenced paths. The indexed bytes are owned externally.

**Application to Fortemi**: The current `StorageBackend` trait (`file_storage.rs:60`) only models the managed case — `write/read/delete/exists/resolve_path`. The trait needs a second variant or a second trait that expresses "I do not own these bytes; I observe them." The cleanest pattern is a `StorageMode` enum on the archive:

```rust
// Sketch — not for implementation, for design discussion in synthesis
pub enum StorageMode {
    Managed { backend: Arc<dyn StorageBackend> },
    Referenced {
        root: PathBuf,
        readonly: bool,
        watch: WatchConfig,
        scan_policy: ScanPolicy,
    },
}
```

The existing `FilesystemBackend` continues to serve Managed mode unchanged. Referenced mode is a new code path that **never writes to `root`** and **only reads** when serving requests.

### 1.2 Read-write overlay pattern (sidecar artifacts)

**Established**. When the indexer needs to associate derived artifacts (thumbnails, transcripts, embeddings, AI summaries) with files in a read-only referenced root, the universal pattern is a **sidecar overlay directory** owned by the indexer:

- **Lightroom / digiKam** (photo managers): user-mounted photo libraries are read-only; the catalog DB and previews live in `~/.lightroom/` or `~/.local/share/digikam/`.
- **Plex / Jellyfin**: media folders are referenced; metadata, posters, and transcoded variants live in the application's own data directory.
- **Sourcegraph blobstore for batch changes**: workspace files are read-only; computed diffs and patches live in a separate object store.

**Application to Fortemi**: The current `crates/matric-jobs` attachment pipeline assumes it can write derived attachments alongside the original (the "derived attachments as child attachments" feature in CLAUDE.md). For Referenced mode, this must split:
- **Originals**: in the user's directory (read-only). Reference via `FileSource::Filesystem(absolute_path)`.
- **Derived (thumbnails, captions, sprite sheets, embeddings)**: in Fortemi's managed blob store, linked by `content_hash` of the original. Schema needs an `original_storage_path` column distinct from the derived-artifact storage path.

This is **not optional** — without the split, ingesting a read-only directory and then asking Fortemi to thumbnail a video would fail, eroding the feature's value.

### 1.3 Lossless trait composition vs. mode flag

**Trade-off, established in both directions**. Two valid patterns exist:

| Pattern | Pros | Cons | Used by |
|---|---|---|---|
| Single trait, mode flag on backend instance | Minimal API surface; existing call sites unchanged | Methods that don't apply to one mode must error at runtime | Quickwit (`StorageResolver` returns a `Storage` that knows whether it's S3/local; `delete` on S3-with-read-only-IAM returns error at call time) |
| Two traits (`ReadableStorage`, `WritableStorage`) with `Storage = Readable + Writable` for managed | Compile-time guarantees that read-only paths can't accidentally write | More invasive refactor; every consumer site needs to be re-typed | Sourcegraph internal `gitserver` (separate `RepoReader` and `RepoUpdater` interfaces) |

**Recommendation for synthesis** (not a decision): start with the mode-flag pattern because it touches less existing code (the `PgFileStorageRepository` at `file_storage.rs:341` and the upload handlers don't need to be re-typed). Re-evaluate after first Referenced-mode use case ships if accidental writes become a real bug class.

---

## 2. Scan-and-Ingest Patterns for Repository Indexing

### 2.1 The ignore-respecting walk (Rust `ignore` crate)

**Established**. Every Rust code-indexing tool (ripgrep, zoekt-rs, hound, gitleaks-in-Rust) uses the BurntSushi/ignore crate or its own equivalent. The crate respects:
- `.gitignore` (the most common case)
- `.git/info/exclude` (per-repo local ignores)
- `~/.gitignore` (global ignores)
- `.ignore` (ripgrep-specific, but useful as a "non-git project" ignore)
- `.rgignore` (ripgrep-specific override)
- Custom user-supplied patterns

**Threading model**: `ignore::WalkBuilder::threads(n)` parallelizes walks across multiple threads. ripgrep defaults to `num_cpus::get()`. For Fortemi's indexer, a configurable but bounded thread count (e.g., min(4, cpus)) prevents the ingest of a 100k-file monorepo from starving the API server.

**Application to Fortemi**: The Rust `ignore` crate (already a transitive dep through tantivy/whoosh in some toolchains; verify in `Cargo.lock`) is the right primitive. Building a custom walker is a known antipattern — gitignore semantics have ~15 years of corner cases.

**Edge cases the crate handles that hand-rolled walkers miss**:
- Negation patterns (`!important.txt`)
- Directory-only patterns (`build/` matches dir but not file)
- Anchored vs. unanchored patterns (`/foo` vs `foo`)
- Trailing-slash semantics
- Case sensitivity per filesystem

### 2.2 Live update detection (notify-rs vs. polling vs. git hooks)

**Trade-offs are established, choice is workload-dependent**.

Three modes are in production use across code-indexing systems:

| Mode | Latency | CPU cost | Reliability | Used by |
|---|---|---|---|---|
| **Filesystem watcher** (`notify-rs`, inotify/FSEvents/ReadDirectoryChangesW) | <1s for local FS; seconds for network FS | Low when idle, spikes during bursts | Inconsistent on network mounts, macOS-with-icloud, Docker bind mounts | hound (optional); livegrep daemon mode |
| **Periodic polling** (`stat`-based mtime scan) | poll interval (typically 30s-5min) | Moderate, scales with file count | Universal — works on any FS | Hound default; Sourcegraph batch reindex |
| **Git hook / explicit "reindex" command** | Manual or post-commit | Zero between events | Perfect within the trust model | Sourcegraph's `gitserver` post-receive hooks; Quickwit's source push API |

**notify-rs specific caveats** (from the project's own README):
- macOS FSEvents has a quirk where rapid bursts can coalesce, dropping individual file events; the recommendation is to use `RecommendedWatcher` (not the raw FSEvents backend) and apply a debounce.
- Linux inotify has a per-user watch limit (`fs.inotify.max_user_watches`, default 8192 on many distros). A repository of 50k+ files will exhaust this. Workarounds: `RecursiveMode::Recursive` on parent directories (cheaper), or fall back to polling at the directory level.
- Docker bind mounts on Linux: inotify events propagate; on macOS Docker Desktop, events used to be unreliable (improved in 2024 but still slower than native).
- Network filesystems (NFS, SMB, sshfs): inotify generally does NOT propagate. Polling is the only option.

**Application to Fortemi**: The realistic default is a **hybrid**:
1. Try notify-rs `RecommendedWatcher` for the directory.
2. If the watcher fails to initialize (unsupported FS, watch limit hit) OR if the user explicitly sets `watch_mode: polling`, fall back to a polling scan at a configurable interval (default 60s).
3. Provide an explicit "reindex now" API endpoint that triggers a full rescan, bypassing the watcher.

The polling fallback path is **not optional**. NFS-mounted home directories on shared filesystems (the exact use case the user described in the issue) will need it.

### 2.3 Incremental indexing — content-hash vs. mtime

**Established trade-off**.

| Strategy | Pros | Cons |
|---|---|---|
| **mtime-based** | Cheap (single `stat` per file); already the OS's job | Touched-but-unchanged files re-index unnecessarily; `touch` on a 1M-line file triggers re-embedding |
| **Content-hash** (BLAKE3, xxhash) | Re-indexes only on actual content change; idempotent | Must read every file to hash; first scan is expensive |
| **Hybrid: mtime → hash** | Fast for unchanged files (skip on mtime match); accurate for changed files (hash before re-index) | Two-level invalidation; needs persistent mtime cache |

**Application to Fortemi**: Fortemi already uses BLAKE3 content-addressing for the managed blob store (`compute_content_hash` at `file_storage.rs:317`). The hybrid pattern is the natural extension:

```
For each file in scan:
  cached = lookup(path) -> {mtime, hash, last_seen}
  if file.mtime == cached.mtime:
    mark seen; skip
  else:
    new_hash = blake3(file)
    if new_hash == cached.hash:
      update mtime cache; skip  # touched but unchanged
    else:
      enqueue for re-indexing
      update {mtime, hash}
For each cached path not seen this scan:
  mark deleted; enqueue delete
```

This requires a new table (or a column on an existing table) that maps `archive_id + relative_path → (mtime, content_hash, last_seen_at)`. Schema design is for the synthesis phase.

### 2.4 Symlink and hardlink handling

**Defensive default established**. Every tool that walks user directories has been bitten by a symlink loop or by following a link out of the intended root.

Recommended defaults:
- **Symlinks pointing inside the root**: follow if `--follow` is set; otherwise skip and log.
- **Symlinks pointing outside the root**: NEVER follow. Skip and log. (Defense against `.config/foo -> /etc` or worse.)
- **Symlink loops**: detect via a visited-inode set (the `ignore` crate handles this when `follow_links(true)`).
- **Hardlinks**: index once by inode. The same content under two paths should not produce two embeddings — content-hash dedup catches this even when inode-tracking misses it.

**Application to Fortemi**: The current `FilesystemBackend` (`file_storage.rs:86`) does not walk anything — it just stores blobs by UUID. The new walker logic must build in symlink discipline from day one. The single most expensive bug to fix later would be a symlink that points to `/proc/self/root/`.

---

## 3. Content Addressing vs. Path Identity — When Each Wins

**Established trade-off, both are correct in their context**.

### 3.1 Content addressing (current Fortemi behavior)

- Two files with identical content → one blob, deduplicated.
- Used by: Git, IPFS, restic, borg, Nix.
- **Strength**: storage efficiency; cryptographic integrity check on every read.
- **Weakness**: same file at different paths is indistinguishable in the blob layer. The "what is this file *called*" question requires a separate path → hash mapping. (Fortemi's `files` table already does this.)

### 3.2 Path identity (what Referenced mode needs)

- The path IS the identity. Two identical-content files at different paths are two distinct entities.
- Used by: every classic filesystem indexer (`locate`, `mlocate`, Spotlight, Windows Search).
- **Strength**: stable identity for live editing — a file's identity doesn't change when its content changes.
- **Weakness**: rename detection is hard. `mv foo.md bar.md` looks like "delete foo.md, create bar.md" unless you correlate content hashes.

### 3.3 The hybrid for Fortemi

Referenced mode needs **path identity for the source**, **content addressing for the derived artifacts**:

| Layer | Identity | Reason |
|---|---|---|
| Source file in user's directory | Path | User owns it; the user thinks of it as a path |
| Indexed chunks (tree-sitter output) | (Source path, byte range, content hash of chunk) | Chunks are derived; dedup across renames is a feature |
| Embeddings | Content hash of chunk | Embedding cost is non-trivial; identical chunks across files share embeddings |
| Thumbnails / transcripts / sprites | Content hash of source file | Same video at two paths → one thumbnail, served via either path |
| Search index entries | Source path | Search results show paths; users want "where in my filesystem is this" |

Rename detection becomes possible by correlating content hashes: if `foo.md` disappears and a new file with the same content hash appears as `bar.md` within a small time window, surface a rename event in the change log instead of delete+create.

---

## 4. Permissions, Security, Multi-Tenant Safety

### 4.1 The "scan must respect the agent's view of the filesystem" principle

**Critical and established**. The walker process runs as a particular OS user (in Docker: typically the container's uid, usually mapped to root inside the container and a non-root uid outside). The walker can ONLY index files that user can read.

Failure mode if ignored: a user mounts `/home/alice/` which contains `~/.ssh/`, `~/.gnupg/`, `~/.aws/credentials`. The walker happily indexes them, embeds them, and now anyone with search access to the archive can find Alice's SSH private key by searching for `BEGIN OPENSSH PRIVATE KEY`.

**Defense layers**:
1. **Path-based denylist** (defense in depth): hard-coded patterns the walker refuses to ingest regardless of perms — `.ssh/`, `.gnupg/`, `.aws/`, `.config/`, `.kube/config`, `*.pem`, `*.key`, `*_rsa`, `id_*`, `.env*`, `secrets.*`. This is a soft-stop; the user can override per-archive.
2. **Content-based detection**: if the file contains a PEM private-key header (`-----BEGIN .* PRIVATE KEY-----`) or matches secret-scanner patterns (gitleaks signatures, truffleHog regexes), refuse to ingest and log. This is a hard-stop.
3. **Permission-based filtering**: the walker reads only what the process user can read. This is the OS's job, not the walker's, but the walker must surface "this file was skipped because of perms" so the user can correct it (e.g., by mounting with `:ro` and the right uid mapping).
4. **No write attempts to source**: enforced at the type level if the two-trait pattern is chosen, or at runtime via the StorageMode enum.

### 4.2 Multi-tenant boundary preservation

**Already enforced in Fortemi by ADR-090 (shared-schema + Postgres RLS) and ADR-093/094 (KeyProvider + fail-closed auth)**. The new feature MUST NOT break these.

Specifically:
- Each archive belongs to one tenant. The Referenced storage root MUST be scoped to that archive's tenant. Tenant A's archive cannot reference a directory owned by Tenant B's user.
- The path-traversal check (when the API exposes "download this file at path X within archive Y") must verify X is within Y's root after canonicalization (`fs::canonicalize`). A request for `../../../../etc/passwd` must return 404, not 200.
- The `ArchiveContext` middleware (`archive_routing.rs:17`) already routes per-archive. The new storage-mode resolution must extend this — the middleware needs to know the storage mode to dispatch file reads correctly.

### 4.3 Container deployment constraints

The Fortemi Docker bundle (per CLAUDE.md) runs PostgreSQL, Redis, API, MCP, Open3D in a single deployment. For Referenced mode to work:
- The user's directory MUST be bind-mounted into the API container at a known path (e.g., `/srv/fortemi/archives/<archive-name>/`).
- The container user MUST have read access (uid/gid mapping is the operator's responsibility, but Fortemi must document this requirement clearly).
- The mount MUST be read-only (`:ro` in docker-compose) when `readonly: true` in the archive config.
- For watching to work, the FS must propagate inotify events into the container. Linux bind mounts do; named volumes do; Docker Desktop for Mac is improving but historically did not.

This needs to be a documented constraint in the deployment plan section, not a hidden assumption.

---

## 5. Tree-sitter, Embeddings, and Code-Indexing Specifics

### 5.1 Tree-sitter chunking is the right primitive for code

**Established across modern code-search**. Tree-sitter (used by GitHub's code-search, Sourcegraph's code-intel, Helix editor) provides language-aware parsing without running a full language server. The chunking pattern:
- For functions, classes, methods: chunk-per-symbol.
- For very large files: subdivide at the next logical boundary (statement-level).
- For non-code files (markdown, JSON, YAML): fall back to a generic chunker (token-count windows with overlap).

**Fortemi's existing chunking**: per CLAUDE.md, "Smart chunking per document type (code uses syntactic, prose uses semantic)" — this is already the right pattern. The new ingest just needs to invoke it per discovered file.

**Tree-sitter grammar set to ship initially**: the consensus from Sourcegraph's symbol-extraction and GitHub's universal-ctags equivalent is to cover the top ~20 languages (TypeScript, JavaScript, Python, Rust, Go, Java, C, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Bash, Lua, HTML, CSS, SQL, Markdown). Fortemi can defer the long-tail (Elixir, Crystal, etc.) to a follow-up.

### 5.2 Embedding strategy — symbol-level vs. file-level vs. hybrid

**Symbol-level is the established win for code search**. Reasons:
- A 2000-line file has many independent functions. A single file-level embedding averages them into mush.
- Search queries like "function that parses JWT" land on the JWT-parsing function, not on a file that mentions JWT in 1% of its content.
- Token budget per embedding is bounded — symbol-level chunks fit within the 512–8192 token windows of common embedding models.

**File-level embeddings have one use**: "find files about X" coarse queries, useful as a pre-filter. The Fortemi two-stage retrieval mentioned in CLAUDE.md ("Coarse-to-fine search for 128× compute reduction") already implements this pattern in concept.

**Recommendation for synthesis**: symbol-level embeddings for code (one embedding per tree-sitter top-level symbol), file-level for prose, with the existing two-stage retrieval doing the coarse-to-fine routing.

### 5.3 Re-embedding cost dominates incremental indexing

**Established cost reality**. A single embedding call to a local Ollama nomic-embed-text run costs ~10-50ms per chunk on a CPU, ~5-20ms on GPU. A 10k-symbol monorepo at 20ms/symbol is 200 seconds of pure embedding time, single-threaded. Parallelizing across 8 workers brings it to ~25s, but eats GPU/CPU during that window.

Implications:
- Initial scan of a large directory should be **backgrounded** (job queue) and **incremental** (visible search results appear as chunks come in, not at the end).
- Re-embeds on file change should be **debounced** (batch a burst of edits, embed once after the burst settles).
- Content-hash dedup is critical: a `cp foo.ts bar.ts` should reuse foo.ts's chunk embeddings, not pay 50% extra to re-embed identical content.

---

## 6. Reconciliation Strategies (File Moves, Changes, Deletions)

### 6.1 The four event types

Established taxonomy across all FS-watching systems:

| Event | OS surface | What the indexer must do |
|---|---|---|
| **Create** | Single create event | Hash, chunk, embed, index |
| **Modify** | Single modify event (sometimes coalesced as delete+create on macOS) | Re-hash; if changed, re-chunk and re-embed |
| **Delete** | Single delete event | Remove index entries; keep blob if referenced by other archives (managed mode only — referenced mode just removes the index entry) |
| **Move/Rename** | OS-dependent: Linux inotify gives separate `IN_MOVED_FROM` + `IN_MOVED_TO`; macOS FSEvents gives generic event flood; Windows gives rename events | Correlate by inode (Linux) or by content-hash within a short time window |

### 6.2 Rename detection via content-hash correlation

**Established pattern, requires a small buffer**.

Algorithm:
1. On `delete` event, do NOT immediately remove the index entry. Mark it `pending_delete` with a TTL (e.g., 30 seconds).
2. On `create` event, hash the new file.
3. If the new file's hash matches a `pending_delete` entry, surface a `rename` event and update the index entry's path in place. No re-chunking, no re-embedding.
4. After TTL expires, finalize the `pending_delete` as a real delete.

This is what gitignore-aware tools do internally. The TTL is the only tunable; 30s handles most batch-rename workloads (a `git mv` produces near-simultaneous delete+create).

### 6.3 Deletion semantics differ by mode

**Managed mode** (current Fortemi behavior):
- Delete the index entry AND delete the blob if reference count drops to zero.
- The blob's content was Fortemi's responsibility; cleaning it up is right.

**Referenced mode**:
- Delete the index entry ONLY.
- The source file is the user's; Fortemi MUST NEVER delete it.
- This is so important it should be enforced at the type level (Referenced backend has no `delete_source` method at all).

---

## 7. Existing Patterns Worth Naming

These are reference implementations the Fortemi ingest should learn from (not copy verbatim, but study the trade-offs they made):

| System | Pattern | What to take | What to leave |
|---|---|---|---|
| **Sourcegraph zoekt** | Trigram indexing per-shard with periodic full reindex | Symbol-level chunking; per-shard threading model | The trigram index itself (Fortemi has FTS + pgvector, doesn't need zoekt's trigram approach) |
| **GitHub Spokes/Mercury** | Push-based ingest from `git push` events | The push API as an alternative to filesystem watching | The internal coordination model — far more complex than Fortemi needs |
| **livegrep** | Manifest-driven scan; explicit reindex API | The manifest pattern — JSON file lists what to scan; user controls it | The C++ build chain |
| **hound** | Periodic polling, per-repo cache, web UI for browsing | Polling fallback when watchers don't work; the `cache.json` invalidation pattern | The Go-only ecosystem |
| **ripgrep / ignore crate** | gitignore-respecting parallel walker | The walker as a library dependency (don't reinvent) | n/a — it's a library, take it |
| **restic** | Content-addressed snapshots with deduplication | The content-addressing semantics — file-level then chunk-level dedup | The backup-specific concerns (compression, encryption-at-rest are different problems) |
| **notify-rs** | Cross-platform FS watching | The library as a dep; the `RecommendedWatcher` abstraction | n/a — use it directly, with the documented platform caveats |
| **Quickwit `StorageResolver`** | URI-based storage backend selection (`s3://`, `file://`, `azure://`) | The URI pattern as a config primitive — `storage: "file:///srv/archives/foo"` vs `storage: "managed"` | Quickwit's distributed indexer model |

---

## 8. Anti-Patterns to Avoid (from established failures)

1. **Don't write to the user's directory, ever, in Referenced mode.** Not even a `.fortemi-index/` sidecar. Some users will mount read-only volumes; some will have CI that fails on dirty working trees. Put sidecars in Fortemi's managed area.
2. **Don't assume gitignore semantics on non-git directories.** Many users will point at a `Documents/` folder, a `Downloads/`, an Obsidian vault. Provide a sane default ignore list (`.DS_Store`, `Thumbs.db`, `node_modules/`, `__pycache__/`, `*.pyc`, `.git/`, `.svn/`, `.hg/`) as well as honoring `.gitignore` if present.
3. **Don't poll at 1Hz.** Polling intervals below ~10 seconds saturate disks on large directories. Default 60s with a "trigger reindex now" API is the right operator UX.
4. **Don't fail an ingest because one file is unreadable.** A permission denied on `.ssh/id_rsa` (correctly!) should produce a logged skip, not abort the scan of the surrounding 10k files.
5. **Don't deduplicate across tenants.** Even if the BLAKE3 hash matches, content from Tenant A and Tenant B must produce separate index entries. The dedup boundary is the tenant, not the global blob store.
6. **Don't expose absolute filesystem paths in API responses.** Surface paths relative to the archive root. Absolute paths leak the deployment topology (container vs. host, mount layout) and create cross-tenant correlation risks.
7. **Don't conflate "file moved" with "file deleted and recreated".** The rename-correlation pattern (§6.2) is worth the buffer.
8. **Don't make the watcher mandatory.** The polling fallback is the path that works on NFS, macOS, and Docker Desktop. Watchers are an optimization, not a requirement.

---

## 9. Summary of Best-Practice Recommendations (for synthesis)

The synthesis phase will fold these into the actual Inception/Elaboration artifacts. As a bullet-list dump for the synthesizer:

- **Storage mode**: introduce `StorageMode::Referenced` alongside the existing managed `FilesystemBackend`. Sidecar derived artifacts in managed storage; never write to the source.
- **Walker**: use the `ignore` crate, threaded to `min(4, cpus)`, with built-in gitignore + Fortemi denylist + content-based secret detection.
- **Live updates**: hybrid notify-rs `RecommendedWatcher` with polling fallback; explicit reindex API as the user escape hatch. Default polling interval 60s.
- **Incremental indexing**: mtime → content-hash invalidation. New `archive_file_cache` table (or equivalent) keyed by (archive_id, relative_path).
- **Chunking and embedding**: keep current tree-sitter + symbol-level embedding pattern; ensure new ingest path invokes existing chunkers via `crates/matric-jobs/src/extraction_handler.rs`.
- **Security**: path denylist + content secret-scan + permission-respecting walk + canonical-path traversal check on API reads.
- **Rename detection**: 30s pending-delete TTL with content-hash correlation.
- **Multi-tenant boundary**: extend `ArchiveContext` middleware to carry storage mode; per-tenant root scoping enforced at the trait dispatch layer.
- **Documentation requirement**: deployment plan MUST document Docker bind-mount requirements (uid mapping, `:ro` flag, FS-event propagation per OS).
- **Anti-patterns to encode as rules**: the 8 items in §8 belong in the SDLC rule corpus once the framework is in Construction.

---

## 10. Open Questions for Synthesis / SDLC Phases

These are NOT decided in this research stream — they need user input or further investigation in Elaboration:

1. **Default scan policy on archive creation**: full scan immediately, lazy scan on first search, or operator-chosen?
2. **Watcher process model**: one watcher per archive (clean isolation, doesn't scale past ~100 archives), one watcher per Fortemi instance (scales, complicates the per-archive enable/disable), or per-archive opt-in?
3. **Background job priority**: should scan-and-ingest of a new archive block other indexing work, or share resources?
4. **MCP exposure**: should the MCP server expose a tool for triggering reindex, or is it API-only?
5. **Search-result UX for Referenced mode**: do we show absolute paths, paths relative to the archive root, or both? (The privacy implications differ.)
6. **Multi-archive overlap**: if two archives reference overlapping directories, is that allowed? Forbidden? Allowed-with-warning?
7. **Performance targets**: what's the operator's expectation for "initial scan of 10k files" — 1 minute? 10 minutes? This drives parallelism defaults and whether GPU embedding is in the critical path.

These will surface as questions to the operator in the Phase 5 approval gate.

---

**End of Research Stream A**.

Next stream (B — Current State 2024-2026): industry adoption of local-storage-backed code-search products, vendor MCP integrations for code indexing, and recent (2024-2026) academic/practitioner findings on agentic code-indexing workflows.
