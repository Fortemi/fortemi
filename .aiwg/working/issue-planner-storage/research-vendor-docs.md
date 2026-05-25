# Research Stream C: Fortemi Source Survey + Vendor Docs

**Issue**: fortemi/fortemi#736 — Local directory / mount as archive storage backend with on-add scan-and-ingest for code indexing
**Stream**: C (of A=best-practices, B=current-state, C=source+vendor)
**Date**: 2026-05-21

---

## Part 1: Fortemi Source Survey

### 1.1 `crates/matric-db/src/file_storage.rs` (1718 lines)

This file is the **central integration point** for the Referenced storage mode. The existing abstractions are already well-positioned — most of the work is *adding a third storage mode* rather than restructuring.

#### `StorageBackend` trait (line 60)

```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn write(&self, path: &str, data: &[u8]) -> Result<()>;
    async fn read(&self, path: &str) -> Result<Vec<u8>>;
    async fn delete(&self, path: &str) -> Result<()>;
    async fn exists(&self, path: &str) -> Result<bool>;
    fn resolve_path(&self, _path: &str) -> Option<PathBuf> { None }  // already exists!
}
```

**Significant**: `resolve_path()` already returns an optional `PathBuf` for backends that can resolve to an on-disk path. This is the existing escape hatch for streaming file serving. A Referenced-mode backend would simply implement `resolve_path()` to return the user's original path verbatim — *no copy needed*.

What needs to change for Referenced mode:
- **`write()` must be gated** — for Referenced archives, the backend should refuse writes or treat them as a no-op (since we don't own the files). This is the largest behavioral shift.
- **`delete()` must be gated** — for Referenced archives, deletion of a blob row should NOT delete the underlying file (we don't own it).
- **`read()` and `exists()` work as-is** — read-only operations against an absolute path are equivalent regardless of whether we "own" the file.

What stays the same:
- Trait surface (no breaking changes)
- `FilesystemBackend` keeps its identity as the Managed-mode backend
- `resolve_path()` semantics already match what Referenced mode needs

#### `FilesystemBackend` (lines 86–217)

- Constructor takes a `base_path: PathBuf` and prepends it to all paths via `full_path()` (line 175). For Referenced mode, paths would be **absolute and not prepended**.
- `sweep_temp_files()` (line 108) walks `{base_path}/blobs/` looking for stale `.bin.tmp` — irrelevant to Referenced (no temp writes happen there).
- `validate()` (line 183) round-trip-tests write/read/delete at startup. For Referenced backends, a corresponding validation would test *readability* and *path-existence* but not writability.
- Atomic write logic (lines 218–289) — full POSIX-correct write+fsync+rename+dir-fsync. Entirely irrelevant to Referenced mode.

#### `compute_content_hash()` (line 317) and `generate_storage_path()` (line 327)

```rust
pub fn compute_content_hash(data: &[u8]) -> String {
    let hash = blake3::hash(data);
    format!("blake3:{}", hash.to_hex())
}
```

- **In-memory only** (`&[u8]`). For Referenced storage of large source files, we need a **streaming** variant: `compute_content_hash_stream(path: &Path) -> Result<String>` using `blake3::Hasher::update_reader()` or similar.
- `generate_storage_path()` produces the `blobs/AA/BB/{uuid}.bin` shard path. **Not used** for Referenced mode — the blob's `storage_path` IS the absolute path the user gave us.

#### `PgFileStorageRepository` (line 341)

```rust
pub struct PgFileStorageRepository {
    pool: PgPool,
    backend: Box<dyn StorageBackend>,
    inline_threshold: i64,  // unused, kept for API compat
}
```

- Single backend per repository today. For multi-mode support, the **storage_mode is per-blob (or per-archive)**, not per-repository. The repository needs to:
  1. Dispatch read paths based on `attachment_blob.storage_backend` (already does — see line 477)
  2. *Refuse* `store_file()` calls for Referenced archives (or route them to a separate ingest path that records metadata without copying)
  3. Skip the orphan-file-deletion step (`self.backend.delete(...)` at line 574) when the blob is `Referenced`-backed

`store_file()` (line 387) and `store_file_tx()` (line 918) currently:
1. BLAKE3-hash the in-memory data
2. Check for existing blob (dedup)
3. If new: write to filesystem backend + `INSERT INTO attachment_blob (storage_backend='filesystem', storage_path=<sharded>)`

For Referenced mode, the equivalent is a **scanner-driven** flow:
1. Scanner walks user-specified directory
2. For each file: BLAKE3-hash via streaming read
3. Check for existing blob
4. If new: `INSERT INTO attachment_blob (storage_backend='referenced', storage_path=<absolute>)` — NO copy
5. Create attachment record linking to a generated note

`download_file_tx()` (line 1019) and `get_file_metadata_tx()` (line 1057) already dispatch on `storage_backend` (`"database"` vs `"filesystem"`). They need a third branch: `"referenced"`. Since `resolve_path()` for a Referenced backend returns the literal absolute path, `read()` works without change.

#### `FileSource` enum (line 49) — the existing precedent

```rust
pub enum FileSource {
    Inline(Vec<u8>),       // database-stored
    Filesystem(String),    // filesystem-stored (path is *relative* to backend base_path)
}
```

This is the model for Referenced mode. We have two clean options:

**Option A** — Add a third variant:
```rust
pub enum FileSource {
    Inline(Vec<u8>),
    Filesystem(String),    // relative path, in Managed blob store
    Referenced(PathBuf),   // absolute path, user-owned
}
```

**Option B** — Reuse `Filesystem(String)` with an absolute path and let `resolve_path()` discriminate via the backend type.

Option A is more explicit, prevents accidentally treating Referenced files as Managed (e.g., the orphan-cleanup logic). Recommend Option A.

#### `FileDownloadInfo` (line 41)

```rust
pub struct FileDownloadInfo {
    pub content_type: String,
    pub filename: String,
    pub size_bytes: u64,
    pub source: FileSource,
}
```

If Option A above is chosen, this naturally carries the discrimination through the API surface to the streaming download code in `crates/matric-api/src/main.rs` (lines 14406, 14443, 14773, etc.).

#### Sweep/cleanup logic (line 108)

`sweep_temp_files()` is Managed-mode-only and needs no change. But a NEW background job is needed for Referenced mode: **integrity sweep** — periodically walk all `attachment_blob` rows where `storage_backend='referenced'` and verify the file still exists, hash still matches, or flag as `quarantined` per #491 missing-file handling. (See Stream A for the integrity-sweep recommendation.)

---

### 1.2 `crates/matric-db/src/archives.rs` (1356 lines)

#### `PgArchiveRepository` (line 118)

The archive repository today is purely a **schema-creation engine** — it clones `public` into `archive_<name>` via `LIKE INCLUDING ALL`, replays FKs and triggers, and seeds the default embedding set + concept scheme. It has *no concept of storage backend* at the archive level — every archive today implicitly uses the global `PgFileStorageRepository`.

Critical insight for #736: **storage_mode is naturally an archive-level property, not a blob-level property** in our domain. A "Referenced archive" is one where ALL blobs are mode=referenced. A "Managed archive" is one where ALL blobs are mode=filesystem (or inline-database for legacy). The data model could express this either way (per-blob vs per-archive), but the user mental model is per-archive.

#### `ArchiveRepository` trait (around line 936)

```rust
async fn create_archive_schema(&self, name: &str, description: Option<&str>) -> Result<ArchiveInfo>;
async fn drop_archive_schema(&self, name: &str) -> Result<()>;
async fn list_archive_schemas(&self) -> Result<Vec<ArchiveInfo>>;
async fn get_archive_by_name(&self, name: &str) -> Result<Option<ArchiveInfo>>;
async fn get_archive_by_id(&self, id: Uuid) -> Result<Option<ArchiveInfo>>;
async fn get_default_archive(&self) -> Result<Option<ArchiveInfo>>;
async fn set_default_archive(&self, name: &str) -> Result<()>;
async fn update_archive_metadata(&self, name: &str, description: Option<&str>) -> Result<()>;
async fn update_archive_stats(&self, name: &str) -> Result<()>;
async fn sync_archive_schema(&self, name: &str) -> Result<()>;
async fn clone_archive_schema(&self, source_name: &str, new_name: &str, description: Option<&str>) -> Result<ArchiveInfo>;
```

Additions needed:
```rust
async fn create_referenced_archive(
    &self,
    name: &str,
    description: Option<&str>,
    source_path: &Path,
    scan_options: ScanOptions,  // gitignore-respecting, max-depth, follow-symlinks, etc.
) -> Result<ArchiveInfo>;
```

Or, more orthogonally: extend `create_archive_schema()` to take an optional `StorageMode { Managed, Referenced { source_path: PathBuf, scan_options: ScanOptions } }`.

#### Schema model — storage_path field today

Today the archive's "storage location" is implicit (it's the global `FILE_STORAGE_PATH` env var). The `archive_registry` table (see migration `20260201220000_archive_registry.sql`) likely has columns:

- `id, name, schema_name, description, created_at, last_accessed, note_count, size_bytes, is_default, schema_version`

For Referenced archives, we need NEW columns:
- `storage_mode TEXT NOT NULL DEFAULT 'managed'` — values: `managed`, `referenced`
- `source_path TEXT NULL` — absolute path on disk (NULL for managed archives)
- `scan_config JSONB NULL` — gitignore patterns, max depth, follow_symlinks, watch_for_changes, etc.
- `last_scan_at TIMESTAMPTZ NULL`
- `scan_status TEXT NULL` — `idle`, `scanning`, `error`, etc.

Plus on `attachment_blob`: the existing `storage_backend TEXT` field accepts a new value `referenced`, and `storage_path` will be an absolute path for referenced blobs. **No schema change needed at the blob level** if we follow the discriminator-by-string pattern already in use.

#### Multi-tenant isolation — how it integrates

Archives are PostgreSQL schemas (`archive_<sanitized_name>`). Cross-archive data isolation is enforced by `SET LOCAL search_path TO archive_X, public` per transaction via `SchemaContext` (see `schema_context.rs`). Referenced mode adds **filesystem-level isolation requirements**:

- The `source_path` field should be validated against an allowlist (env var `FORTEMI_REFERENCED_STORAGE_ROOTS` or per-deployment config) to prevent users from referencing arbitrary system paths.
- For multi-tenant deployments (`FORTEMI_MULTI_TENANT=true`), the source_path must be **tenant-scoped** — likely by requiring it to be under a tenant-specific root directory.
- Path traversal protection: canonicalize paths and reject any `..` segments at validation time.

#### Archive lifecycle

`create_archive_schema()` (line 942) creates schema + tables atomically. For Referenced mode, after schema creation we need to:
1. Validate `source_path` exists and is readable
2. Validate path against allowlist (if configured)
3. Queue an initial **scan-and-ingest job** to populate the archive with blobs+attachments+notes
4. Optionally enable filesystem watching (notify-rs)

`drop_archive_schema()` (line 1000) drops the PG schema CASCADE. For Referenced archives this is **safe by default** — we don't own the files, so dropping the archive does NOT touch the source path. The existing code that deletes `job_queue` rows referencing notes (line 1017) works equivalently. **No physical-file deletion logic to gate**, because for Referenced archives we never called `backend.delete()` in the first place.

---

### 1.3 `crates/matric-api/src/middleware/archive_routing.rs`

#### `ArchiveContext` (line 17)

```rust
pub struct ArchiveContext {
    pub schema: String,
    pub is_default: bool,
    pub name: Option<String>,
}
```

For Referenced mode, this needs a `storage_mode` field so downstream handlers know whether they're operating on a Managed or Referenced archive (which affects whether write operations are permitted via the API):

```rust
pub struct ArchiveContext {
    pub schema: String,
    pub is_default: bool,
    pub name: Option<String>,
    pub storage_mode: StorageMode,  // NEW
    pub source_path: Option<PathBuf>,  // NEW: for Referenced archives
}
```

#### `DefaultArchiveCache` (line 44)

Today caches the default archive's `(schema, is_default, name)` with TTL expiration. Needs to also cache `storage_mode` and `source_path`. Refresh logic (`refresh_and_get`, line 85) needs to populate from the new `archive_registry` columns.

#### Middleware behavior changes

In `archive_routing_middleware()` (line 166), after resolving the archive:
- If `storage_mode == Referenced`, the request handler should **refuse write operations** (POST/PUT/DELETE on notes/attachments) with a clear 403 error explaining the archive is read-only.
- Read operations work unchanged — they hit the same database via the same schema.
- File downloads route through `resolve_path()` exactly as today, just with absolute paths.

---

### 1.4 `crates/matric-jobs/src/extraction_handler.rs` (1872 lines)

#### Job dispatch and pipeline

The extraction handler dispatches by `ExtractionStrategy` (line 99): TextNative, PdfText, PdfOcr, Vision, AudioTranscribe, VideoMultimodal, **CodeAst** (already exists!), OfficeConvert, StructuredExtract, Glb3DModel.

**CodeAst already exists** as `crates/matric-jobs/src/adapters/code_ast.rs` — but uses regex-based detection (line 1: "regex-based declaration detection"), not actual tree-sitter parsing. The Cargo manifest shows `tree-sitter`, `tree-sitter-rust`, `tree-sitter-python`, `tree-sitter-javascript`, `tree-sitter-typescript` as **optional** dependencies behind a `tree-sitter` feature flag. For #736's "on-add scan-and-ingest for code indexing" requirement, we have two paths:

- **Minimum viable**: use the existing regex-based CodeAstAdapter for initial release (works today, no new deps)
- **Full**: upgrade CodeAstAdapter to actually use tree-sitter when feature flag is enabled (a follow-on enhancement, ties into broader #-TBD tree-sitter activation)

#### Where the scan-and-ingest job hooks in

A new `JobType::DirectoryScan` (or `ArchiveScan`) is needed. The handler flow:

```
1. Walk source_path with `ignore::WalkBuilder` (respects .gitignore/.fortemiignore)
2. For each file:
   a. Stream-BLAKE3 to compute content_hash (no full-file load)
   b. Detect MIME type via `infer` crate (already in deps)
   c. INSERT attachment_blob (storage_backend='referenced', storage_path=<absolute>)
   d. Create stub note (filename + path)
   e. INSERT attachment linking note → blob
   f. Queue Extraction job (existing pipeline takes over)
3. Update archive_registry.last_scan_at, scan_status='idle'
```

This piggybacks on the **existing extraction pipeline** — the Extraction job reads from the storage backend, which for referenced blobs is `resolve_path()` → absolute path → adapter reads it. The pipeline's "supports_path_access" optimization (line 146) already exists for VideoMultimodal and AudioTranscribe, and applies cleanly here since CodeAstAdapter, PdfText, etc. can all benefit from direct path access without copying.

#### Multimodal extraction paths and original-file access

Lines 146–217: the extraction handler already has a **"path access"** code path for video/audio that *bypasses the in-memory download* and passes the storage path directly to the adapter via `config._source_path`. This is exactly what Referenced mode needs — and it's already wired up!

For Referenced archives, ALL strategies should ideally use path-access mode (since we never want to copy user files into memory unnecessarily). The current logic gates path-access by strategy (line 146); it should additionally gate by `storage_backend == 'referenced'` → always-use-path.

#### Side-effect concerns for Referenced mode

The extraction handler today writes **derived attachments** (transcripts, thumbnails, captions, keyframes) via `store_derived_attachment_tx()` (line 770). For Referenced archives, these derived artifacts MUST go somewhere — they cannot be written next to the user's source files. Two options:

- **A**: Derived artifacts go to a **companion managed location** per archive (e.g., `{FILE_STORAGE_PATH}/derived/{archive_id}/`). Simple, isolates derived from source.
- **B**: Derived artifacts stay in the database (inline) for Referenced archives. Avoids any filesystem write at all but increases DB bloat for video/audio archives.

Recommend Option A. The derived attachments would be `storage_backend='filesystem'` even in a Referenced archive — i.e., **mixed-mode archives** where the source blobs are Referenced but derived blobs are Managed.

---

### 1.5 Cargo workspace dependencies (from `Cargo.toml` workspace section)

**Already present:**
- `blake3 = "1.5"` — full crate, ready for streaming via `blake3::Hasher`
- `tree-sitter = "0.24"` + grammars for `rust, python, javascript, typescript` (already at v0.23 grammars) — present but optional behind feature flag in matric-jobs
- `tokio` with `rt-multi-thread, macros, sync, time` — `fs` and `process` features added per-crate
- `pgvector = "0.4.1"` with sqlx+postgres features
- `infer = "0.16"` — MIME detection
- `image = "0.25"` — jpeg/png/gif/webp
- `tempfile` (used in tests + extraction handler)
- `sqlx = "0.8.6"` with postgres+uuid+chrono+json+macros+bigdecimal+migrate

**MISSING — needed for #736:**
- `ignore` crate (BurntSushi) — gitignore-respecting walker. **Required.**
- `notify` (or `notify-debouncer-full`) — cross-platform filesystem watcher. **Required only if we ship live file watching in v1; can defer.**
- `walkdir` — basic walker without gitignore. Probably not needed if we have `ignore`.

---

## Part 2: Vendor Docs / Library References

### 2.1 BurntSushi/ignore (gitignore-respecting walker)

- **Crates.io**: https://crates.io/crates/ignore
- **Docs**: https://docs.rs/ignore/latest/ignore/
- **Key API**:
  - `WalkBuilder::new(path).build()` — synchronous iterator
  - `WalkBuilder::new(path).build_parallel()` — multi-threaded for large trees
  - `.add_custom_ignore_filename(".fortemiignore")` — supports project-specific ignore files alongside `.gitignore`
  - `.standard_filters(true)` — enables `.gitignore`, `.git/info/exclude`, global git excludes, hidden-file filter
  - `.max_depth(Some(n))`, `.follow_links(bool)`, `.hidden(bool)`
- **2024–2026 caveats**:
  - Stable API since 0.4; current is `0.4.23` (Apr 2025) — no breaking changes recently
  - For 10k–100k file trees, `build_parallel()` with a `visitor_builder` is the appropriate scaling pattern
  - Symlink-loop protection is on by default (good)
- **Reference docs that matter for #736**: the `WalkBuilder` and `Walk` types; `DirEntry::file_type()` and `DirEntry::path()`; `Error` taxonomy for handling permission-denied subdirs gracefully (which is critical for code repos under restricted user accounts)

### 2.2 notify-rs (cross-platform filesystem watcher)

- **Crates.io**: https://crates.io/crates/notify
- **Companion**: `notify-debouncer-full` (recommended for production — handles the "saved 50 events for one git checkout" problem)
- **Docs**: https://docs.rs/notify/latest/notify/
- **Key API**:
  - `RecommendedWatcher::new(handler, Config::default())?`
  - `Watcher::watch(path, RecursiveMode::Recursive)?`
  - Events: `Create`, `Modify`, `Remove`, `Rename`
- **2024–2026 caveats** (CRITICAL):
  - **macOS FSEvents quirks**: FSEvents can deliver events with a >100ms delay and may coalesce or even drop events under load. The `kqueue` backend is more responsive but doesn't recurse. Most production users on macOS use the `fsevent-sys` default and accept the latency.
  - **Linux inotify limits**: Default `fs.inotify.max_user_watches` is 8192 on many distros — trivially exhausted by a 50k-file project. Document this in operator docs and recommend `sysctl fs.inotify.max_user_watches=524288`. Inotify also does NOT recurse — notify-rs handles recursion in userspace, costing one watch descriptor per directory.
  - **Docker bind-mount behavior**: inotify events do not propagate from host to container reliably across all storage drivers. Specifically: `overlay2` works for files modified inside the container but NOT for files modified on the host. For Fortemi's bundle, this means live-watch on a `/host/source/path` bind-mount may silently miss host-side edits. **Recommend polling fallback for Docker deployments.**
  - **Windows**: ReadDirectoryChangesW is reliable but has a 64KB event buffer that overflows under heavy churn (e.g., `npm install` extracting 10k files). The `notify-debouncer-full` crate solves this for most cases.
- **Recommendation for v1**: ship without live watch. Add it in a follow-up issue with platform-specific behavior documented.

### 2.3 tree-sitter (language-aware parsing)

- **Crates.io**: https://crates.io/crates/tree-sitter (Fortemi tracks `0.24`)
- **Docs**: https://docs.rs/tree-sitter/0.24/tree_sitter/
- **Languages active in Fortemi** (per workspace Cargo.toml):
  - `tree-sitter-rust = 0.23`
  - `tree-sitter-python = 0.23`
  - `tree-sitter-javascript = 0.23`
  - `tree-sitter-typescript = 0.23`
- **Key API**:
  - `Parser::new()` → `set_language(&Language)` → `parse(source, None)` → `Tree`
  - `Tree::root_node()` for AST root
  - `Query::new(&Language, query_string)` for s-expression queries (the right way to extract declarations)
- **2024–2026 caveats**:
  - tree-sitter 0.24 introduced breaking changes from 0.20 — grammar crates must be on matching 0.23+ versions (Fortemi is already aligned)
  - The C ABI between `tree-sitter` and grammar crates is version-locked; mixing 0.22 grammars with 0.24 core will not compile
  - For 100k-LOC repos, parser instances should be **per-thread, not global** — `Parser` is not Sync (parse state is mutable)
- **For #736**: the existing CodeAstAdapter (`crates/matric-jobs/src/adapters/code_ast.rs`) uses regex today. An enhancement to use real tree-sitter parsing is a sensible follow-on but **not blocking** for #736 — regex extraction works for Inception/Elaboration MVP.

### 2.4 pgvector (vector indexing)

- **Crates.io**: `pgvector = "0.4.1"` with `sqlx, postgres` features (already in Fortemi)
- **Postgres extension docs**: https://github.com/pgvector/pgvector
- **HNSW vs IVFFlat for Referenced archives at 10k–100k document scale**:
  - **HNSW** (Hierarchical Navigable Small World): build-time slower (O(N log N)), query-time much faster, no training set needed, recall typically >95% at default params. **Default choice for <1M vectors.**
  - **IVFFlat**: build-time faster but requires `lists = sqrt(N)` clusters and a representative training set; works better at >1M scale.
  - For a 10k–100k-document code archive (assuming ~10–50 chunks per file → 100k–5M vectors), HNSW with `m=16, ef_construction=64` is the recommended default. IVFFlat becomes competitive past ~1M vectors.
  - Fortemi already uses pgvector for the `embedding` table (per archive). The Referenced archive uses the same index strategy — no schema change needed for indexing itself, just the ingest path.
- **Memory caveat**: HNSW index memory is non-trivial. For 1M 768-dim vectors at default params, expect ~3–4GB RAM. Document this in operator docs.

### 2.5 PostgreSQL Row-Level Security (RLS)

- **Postgres docs**: https://www.postgresql.org/docs/16/ddl-rowsecurity.html
- **Fortemi's RLS posture**: Fortemi uses **schema-level isolation** (one PG schema per archive) plus `SET LOCAL search_path` per transaction (see `crates/matric-db/src/schema_context.rs`). It does NOT currently use RLS — isolation is enforced by the schema boundary itself, which is a stronger guarantee for the multi-tenant default-deny pattern Fortemi targets.
- **ADR-090 reference** (mentioned in memory): I could not find an explicit ADR-090 file in the working tree. The closest analogue is the multi-memory architecture described in `CLAUDE.md` ("Multi-Memory Architecture" section) which describes per-memory schemas + `SchemaContext` + `SET LOCAL search_path`. ADR-090 may be a planned future ADR or live in `.aiwg/architecture/`.
- **For #736**: adding a `storage_mode` column to `archive_registry` is in the **shared** `public` schema (`archive_registry` is in the `SHARED_TABLES` deny list per `archives.rs:101`). It is not subject to per-archive RLS today and doesn't introduce new RLS requirements. The existing `archive_routing_middleware` is the enforcement point for read/write distinction.

### 2.6 blake3 crate (content addressing)

- **Crates.io**: `blake3 = "1.5"` (already in Fortemi)
- **Docs**: https://docs.rs/blake3/latest/blake3/
- **Streaming API** (the one we need for large source files):
  ```rust
  let mut hasher = blake3::Hasher::new();
  hasher.update_reader(File::open(path)?)?;  // 1.4+
  // OR for tokio:
  let mut hasher = blake3::Hasher::new();
  let mut file = tokio::fs::File::open(path).await?;
  let mut buf = vec![0u8; 64 * 1024];  // 64KB chunks
  loop {
      let n = file.read(&mut buf).await?;
      if n == 0 { break; }
      hasher.update(&buf[..n]);
  }
  let hash = hasher.finalize();
  ```
- **Performance**: BLAKE3 is faster than SHA-256 by 5–10x on modern x86 (AVX-512) and ARMv8 (NEON+crypto). For a 100MB file, expect <100ms hash time on commodity hardware. Multithreaded `Hasher::update_rayon()` is also available via the `rayon` feature for very large files (>1GB) — probably not needed for code repos.
- **2024–2026 caveats**: stable API since 1.0; no recent breakage.

### 2.7 MCP (Model Context Protocol) — Anthropic 2025–2026 spec

- **Spec**: https://spec.modelcontextprotocol.io/specification/
- **Fortemi's MCP server**: `mcp-server/` directory, Node.js (per CLAUDE.md), 43 core tools in default mode
- **Tool signature for "trigger reindex of archive X"** — the standard MCP pattern (2025-06-18 spec):
  ```typescript
  {
    name: "rescan_archive",
    description: "Rescan a Referenced archive's source directory for added/modified/deleted files.",
    inputSchema: {
      type: "object",
      properties: {
        archive_name: { type: "string", description: "Name of the Referenced archive to rescan" },
        full: { type: "boolean", default: false, description: "Force full rescan (default: incremental)" }
      },
      required: ["archive_name"]
    }
  }
  ```
- The MCP `tools/call` handler would queue an async job (returning the job_id immediately, since rescans of 100k-file repos can take many seconds) and the caller polls via `get_job_status`. Aligns with the existing `manage_jobs` and `bulk_reprocess_notes` tools per CLAUDE.md core tool list.
- **2025–2026 caveats**: MCP spec is stable as of 2025-06-18 revision. Streaming responses via SSE are now standard; Fortemi's existing MCP server should already speak this.

---

## Part 3: Concrete Extension Points

The minimum-viable Referenced storage mode requires changes to these files. Risk levels reflect blast radius across the existing test suite + multi-tenant boundary.

| File | Change type | What changes | Risk |
|---|---|---|---|
| `migrations/<new>_referenced_storage.sql` | new | Add `storage_mode TEXT NOT NULL DEFAULT 'managed'`, `source_path TEXT`, `scan_config JSONB`, `last_scan_at TIMESTAMPTZ`, `scan_status TEXT` to `archive_registry`. Add CHECK constraint on storage_mode. | LOW — additive, default value preserves existing archives |
| `crates/matric-db/src/file_storage.rs` | modify | Add `Referenced(PathBuf)` variant to `FileSource` enum; add `ReferencedBackend` impl of `StorageBackend` that no-ops `write`/`delete` and resolves absolute paths in `read`/`resolve_path`. Add streaming `compute_content_hash_stream(path)`. | MEDIUM — touches the central trait but additive |
| `crates/matric-db/src/archives.rs` | modify | Add `StorageMode` enum; extend `ArchiveInfo` and `create_archive_schema()` signature to accept storage_mode + source_path. Add `create_referenced_archive()` convenience method. Update `drop_archive_schema()` to never touch source_path. | MEDIUM — changes core repo trait; needs careful test coverage |
| `crates/matric-api/src/middleware/archive_routing.rs` | modify | Add `storage_mode` and `source_path` to `ArchiveContext` and `DefaultArchiveCache`. Add write-gate: if `storage_mode == Referenced`, reject mutating routes with 403 (or route-level allow-list for jobs). | MEDIUM — middleware is on every request path |
| `crates/matric-api/src/main.rs` | modify | New routes: `POST /api/v1/archives/referenced` (create), `POST /api/v1/archives/{name}/rescan` (trigger scan). Wire `FileSource::Referenced` into existing file-streaming code paths (~10 call sites). | MEDIUM — main.rs is 20k+ lines; touch is localized but surface area visible to clients |
| `crates/matric-jobs/src/<new>_directory_scan_handler.rs` | new | New `JobType::DirectoryScan`. Walks source_path using `ignore::WalkBuilder`, streams BLAKE3, INSERTs blobs/attachments/notes per file, queues Extraction jobs. Idempotent on content_hash dedup. | LOW — new code, follows existing handler patterns |
| `crates/matric-jobs/src/extraction_handler.rs` | modify | Extend the path-access optimization (line 146) to ALWAYS use path-access for `storage_backend='referenced'` blobs, not just Video/Audio. For derived artifacts, route to companion managed storage location. | MEDIUM — extraction pipeline is hot path; ~1800 lines |
| `crates/matric-jobs/src/lib.rs` + `Cargo.toml` | modify | Add `ignore = "0.4"` dependency. Register new `DirectoryScanHandler`. | LOW — additive |
| `crates/matric-core/src/lib.rs` (or types module) | modify | Add `JobType::DirectoryScan` enum variant + `StorageMode` enum. | LOW — additive enum variants |
| `mcp-server/` | modify | Add `rescan_archive` tool + `create_referenced_archive` tool to core tool set. | LOW — additive |

Notional v1 line count: ~1500–2500 lines added/modified, concentrated in `archives.rs`, `file_storage.rs`, new `directory_scan_handler.rs`, and `archive_routing.rs`.

---

## Synthesis Handoff Notes

5 things the synthesizer must not forget when assembling the Inception/Elaboration corpus:

1. **The `resolve_path()` escape hatch already exists.** This is the single most important architectural finding: `StorageBackend::resolve_path()` was designed for streaming downloads but happens to be exactly the right primitive for Referenced storage. Whatever ADR is written about storage abstraction should call this out — the trait does NOT need a breaking change.

2. **Storage mode is per-archive in our domain, even if it's per-blob in the schema.** All UX, all API surface, all SAD diagrams should treat "Referenced" and "Managed" as **archive properties**. Per-blob mixed-mode is technically possible (and is exactly what we need for derived artifacts in a Referenced archive — those derived blobs go to managed storage) but it's an implementation detail, not a user-facing concept.

3. **Derived artifacts are the trickiest detail.** Don't lose sight of this — extraction handlers produce thumbnails, transcripts, captions, keyframes that MUST be stored somewhere. Recommendation: companion managed location per archive (`{FILE_STORAGE_PATH}/derived/{archive_id}/`), making Referenced archives effectively **mixed-mode at the blob level** even though they present as Referenced at the archive level. This is the cleanest separation of "user data we don't own" from "system-generated artifacts we do own."

4. **Live filesystem watching is a v2 feature.** Docker bind-mount inotify behavior is unreliable across storage drivers (overlay2 misses host-side edits), and Linux's default `max_user_watches=8192` is laughably small for code repos. v1 ships with on-demand `POST /rescan` only. Avoid promising real-time watching in Inception/Elaboration artifacts — it's a known cross-platform foot-gun. notify-rs goes into the architecture-evolution backlog, not the v1 SAD.

5. **CodeAst already exists but uses regex, not tree-sitter.** Do not commission new work for code parsing as part of #736's core scope. The existing `CodeAstAdapter` (`crates/matric-jobs/src/adapters/code_ast.rs`) is regex-based and works today for Rust/Python/JS/Go. Upgrading it to real tree-sitter parsing is a separate, parallel issue. The Referenced storage mode + scan-and-ingest pipeline should be designed to work with the existing extraction pipeline AS-IS, which means it will produce regex-quality code structure on day one. The tree-sitter feature flag is wired but unused; flipping it is a follow-on enhancement.
