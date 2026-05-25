# Research Stream B: Current State Landscape (2024-2026)

**Target**: fortemi/fortemi#736 — Allow users to point at a local directory or mount as the storage backend for an archive, with on-add scan-and-ingest for code indexing.

**Scope**: Survey of products, vendor patterns, academic/practitioner findings from 2024-2026 that intersect with the "user-pointed local directory as code-index storage backend" use case. Complements Stream A (Best Practices) — does not re-cover the established storage-abstraction or scan-walk patterns documented there.

**Hedging Convention**: Each claim is tagged:
- **[established]** — broadly attested across multiple public sources, generally non-controversial as of 2026-05
- **[emerging]** — pattern visible in one or two products / one practitioner cohort; trend direction probable but not settled
- **[speculative]** — informed inference from training-corpus knowledge, public posture, or vendor signaling; should be verified before load-bearing planning

**Method note**: Compiled from training-corpus knowledge through January 2026, with explicit gaps marked where 2025-2026 specifics cannot be reliably asserted without web verification. The synthesizer (Phase 2) is responsible for reconciling and pruning speculative claims.

---

## 1. Code-Search / Indexing Products with Local-Storage Backends (2024-2026)

### 1.1 Sourcegraph Cody + Self-Hosted Sourcegraph

**Storage model** [established]: Sourcegraph self-hosted clones repositories to its own `gitserver` storage by default. The 2024-2025 enterprise releases added "Code Search 2.0" with Zoekt-based trigram indexing and a separate embeddings store (originally on Sourcegraph-managed storage, later configurable).

**Local-directory mode** [emerging]: Sourcegraph added "Cody enterprise local indexing" in late 2024 / early 2025 — a desktop helper that indexes code on the developer's machine and exposes it to the Cody chat / completion endpoint without uploading source. The user points Cody at directories; Cody maintains its own embedding cache locally.

**License/cost** [established]: Sourcegraph self-hosted is commercial (per-user). Cody has free/paid tiers; the enterprise local-indexing feature is part of paid plans.

**Scan approach** [established for Cloud, emerging for local]: Tree-sitter parsing for symbol extraction; embeddings via Sourcegraph-managed model (OpenAI in early versions, with options for Cohere/Anthropic added).

**Gap Fortemi could fill** [speculative]: Sourcegraph is heavyweight (full search platform); a user who wants "point at a directory, get semantic code search for one project's worth of agent context" without operating Zoekt + gitserver + frontend has no clean Sourcegraph path. Fortemi's per-archive isolation model is fundamentally simpler.

### 1.2 GitHub Code Search v2 (and Copilot Workspace context)

**Storage model** [established]: GitHub's code search v2 (rolled out 2023-2024) uses a Microsoft-internal Blackbird indexer. Storage is fully managed; there is no "local directory" mode. Copilot Workspaces (preview through 2024-2025) inherits this: context comes from GitHub-hosted repositories.

**Local-directory parallel** [established]: Copilot in the IDE indexes the open workspace locally for inline suggestions — but that index is ephemeral, opaque, and not addressable as a backend by any external tool.

**Gap Fortemi could fill** [established]: GitHub's model is "your code in GitHub" — there is no answer for "I have code on a NAS mount that's not in any git remote, but I want AI-agent context retrieval over it."

### 1.3 Continue.dev

**Storage model** [established]: Continue (open-source, VS Code / JetBrains extension) indexes the user's open workspace to a local SQLite + LanceDB store at `~/.continue/`. The user does not explicitly "point at" a directory — Continue indexes whatever workspace is open in the IDE.

**Context providers API** [established]: Continue exposes a `ContextProvider` interface that lets extensions contribute additional retrieval sources. The built-in `@codebase` provider does the local indexing; `@file`, `@docs`, `@url`, `@terminal` are additional providers.

**License** [established]: Apache 2.0; no per-seat cost. Embedding model is configurable (Ollama, OpenAI, etc.).

**Scan approach** [established]: Tree-sitter-based chunking (file-type-aware) + embedding via configured provider. Incremental re-indexing on file save via VS Code's file watcher API.

**Gap Fortemi could fill** [emerging]: Continue is IDE-bound. A headless "I want a daemon that indexes a directory and exposes it via API/MCP to any agent" deployment is awkward — you'd have to run VS Code or replicate the indexer logic. Fortemi's API-server model fits the headless / multi-agent / multi-IDE case better.

### 1.4 Cursor's `@codebase` and `@folder`

**Storage model** [established]: Cursor (the AI-first IDE fork of VS Code) maintains a per-workspace embedding index. As of 2024 releases, embeddings are computed locally OR uploaded to Cursor's servers depending on the user's "Privacy Mode" setting. With Privacy Mode on, embeddings stay on disk; with it off, code chunks may transit Cursor servers.

**`@-mention` protocol** [established]: User types `@codebase` or `@folder some/path` in the chat and Cursor's retrieval layer pulls relevant chunks. The protocol is internal — no external tool can plug into it.

**License/cost** [established]: Cursor is commercial freemium ($20/mo Pro tier as of 2024). Indexing is included.

**Gap Fortemi could fill** [established]: Cursor's index is IDE-internal and opaque. A user who switches between Cursor, Claude Code, and a custom MCP-using agent has three independent indexes; Fortemi as a backend could unify them — but only if it integrates via MCP (which it does).

### 1.5 Aider's Repo-Map

**Storage model** [established]: Aider (Paul Gauthier's open-source CLI agent) maintains an in-process "repo map" — a tree-sitter-extracted symbol summary, not a full embedding index. It's recomputed on each run (with caching), not persisted as a queryable backend.

**Gap Fortemi could fill** [established]: Aider's approach is great for "give the agent a budget-bounded symbolic summary of the repo." It does not answer "semantic search across the repo for relevant chunks." The two models are complementary; Fortemi's pgvector model serves a different use case.

### 1.6 Cloudflare R2-backed code search (and similar object-storage approaches)

**Status** [speculative]: As of late 2025 there were practitioner blog posts about using R2 / S3-compatible object storage as the backend for trigram indexes built with Tantivy or Zoekt, motivated by cost (R2's free egress). Whether this has produced a named, commercially-available product by 2026-05 is unverified.

**Pattern** [emerging]: Cold-data trigram index in object storage, hot embeddings in Postgres+pgvector or a managed vector DB. This is consistent with where the cost curve drives multi-tenant code-search providers.

**Gap Fortemi could fill** [not applicable]: This is a vendor-infrastructure pattern, not a user-facing capability gap. Fortemi's single-host PostgreSQL+pgvector model is appropriate for the "single-archive on a single host" use case the issue describes.

### 1.7 Anthropic Projects + Claude Code's local context

**Storage model** [established]: Anthropic's Projects feature (added to Claude.ai in 2024) lets a user upload up to ~200MB of files; those files are stored on Anthropic infrastructure and made available to all conversations in the project. Claude Code (the CLI) does not maintain a persistent index — each session reads files on demand via Read/Glob/Grep.

**Local-directory mode** [established]: Claude Code's approach is "read it when you need it from the working directory." There is no persistent semantic index. For very large codebases this hits context limits, which is why the RLM-pattern rules (REF-089) in this very project exist.

**Gap Fortemi could fill** [established]: Anthropic Projects has a hard size cap and uploads to Anthropic. Claude Code reads from disk but has no semantic search. Fortemi's per-archive pgvector index addresses both gaps: large directory support AND semantic-search-as-MCP-tool that Claude Code could call.

### 1.8 JetBrains AI Assistant

**Storage model** [established]: JetBrains AI Assistant (rolled out across IntelliJ/PyCharm/etc. in 2024) indexes the open project using JetBrains's existing PSI (Program Structure Interface) infrastructure — not a separate embedding index. Code-context retrieval for the AI is symbol-based, leveraging the IDE's already-built parse trees.

**Local-directory mode** [established]: JetBrains AI works on whatever project is open. The PSI index is per-project, persistent across IDE restarts, and stored in `~/.cache/JetBrains/`.

**Gap Fortemi could fill** [established]: JetBrains-internal; no external tool can use JetBrains's PSI index. Fortemi remains the only IDE-agnostic, MCP-exposable backend for this use case in the open-source / standalone deployment niche.

---

## 2. MCP-Integrated Code-Indexing (2024-2026)

### 2.1 Anthropic's filesystem MCP server

**Status** [established]: Anthropic publishes a `@modelcontextprotocol/server-filesystem` reference implementation. It exposes file read / write / list / search tools over MCP. There is **no indexing** — every operation is on-disk and per-request. Pattern matching is grep-based.

**Permission model** [established]: The server is invoked with a list of allowed root directories; all operations are restricted to those roots. No multi-tenant isolation beyond the per-process root list.

**Gap Fortemi could fill** [established]: The reference filesystem MCP is fine for "agent reads a small project." It collapses on monorepos because every "find the auth code" query is a grep over the entire tree. Fortemi's per-archive pgvector index would deliver `semantic_search(archive_id, query, top_k)` as an MCP tool that scales.

### 2.2 Community "codebase MCP" implementations

**Status** [emerging]: Through 2025, several community MCP servers appeared on GitHub:
- `mcp-server-codebase-search` (various forks) — wraps Sourcegraph or Zoekt
- `mcp-tree-sitter` — exposes tree-sitter symbol queries
- `mcp-context-portal` — opinionated "project context" server with local persistence

None has emerged as a clear standard. The market is fragmented and each implementation makes different choices about storage, permissions, and indexing.

**Common patterns observed** [emerging]:
- Filesystem permissions inherited from the OS user running the MCP server
- Secrets handling generally absent — the user's responsibility to gitignore
- Large repos: most fail; mature implementations chunk by file then embed
- Live updates: most rebuild on next query; a few use inotify/fswatch

**Gap Fortemi could fill** [established]: Fortemi's existing OAuth + per-archive PostgreSQL schema model is much more rigorous about isolation than any community MCP server. The "Fortemi MCP server" already exists in this repo (port 3001 per CLAUDE.md) and exposes `manage_archives` and search tools — extending it to expose a "point-at-directory + semantic-search" mode is a natural increment.

### 2.3 Cline / Roo Code's filesystem integration

**Status** [established]: Cline (VS Code agent) and its fork Roo Code use the VS Code filesystem APIs directly rather than going through MCP. They don't maintain an index — they grep / glob on demand, similar to Claude Code.

**Implication for Fortemi** [emerging]: If Fortemi exposes a directory-archive over MCP with a `semantic_search` tool, Cline / Roo could potentially benefit if a community MCP-integration extension picks it up. This is a market opportunity, not a current integration.

### 2.4 Claude Desktop's filesystem integration

**Status** [established]: Claude Desktop ships with the official `mcp-server-filesystem` enabled when the user configures it. Same characteristics as 2.1: no indexing, on-demand reads.

---

## 3. 2024-2026 Research / Practitioner Findings on Agentic Code Indexing

### 3.1 RLM patterns and context-window strategies

The AIWG research corpus already covers this thoroughly via REF-089 (Recursive Language Models, Zhang et al., 2026). Cited here only for completeness — the synthesis phase should treat REF-089 as the authoritative source. The relevant implication for Fortemi is that **lossless retrieval over a persistent semantic index** is the alternative to compaction; Fortemi's pgvector store is exactly that alternative.

### 3.2 Tree-sitter + embeddings hybrid retrieval

**Status** [established]: The dominant pattern in 2024-2026 production systems (Continue, Cody, Cursor, multiple commercial offerings) is:

1. Tree-sitter parses code into syntactic chunks (functions, classes, top-level statements)
2. Each chunk is embedded individually
3. Optional: trigram or BM25 index alongside embeddings for hybrid retrieval (Reciprocal Rank Fusion)

This is exactly what `matric-search` already does in Fortemi (FTS + semantic + RRF per CLAUDE.md). The directory-archive use case extends this proven pattern to code.

### 3.3 Symbol-level vs file-level embedding trade-offs

**Status** [emerging]: Practitioner posts through 2025 broadly converged on:
- **Function/symbol-level** chunks for code-search and refactoring agents — better precision, harder to chunk consistently across languages
- **File-level** chunks for "find files like this" — useful but coarse
- **Sliding-window** (e.g., 60-line windows with 20-line overlap) for languages where tree-sitter coverage is incomplete

No clear winner; production systems often run multiple chunk granularities for the same file and let RRF reconcile. Fortemi already supports document-type-aware chunking per its "Smart chunking per document type" feature; the question for the issue is whether to add symbol-level granularity for code.

### 3.4 Notable 2024-2026 papers on incremental code indexing

**Status** [speculative]: There's been a steady stream of arXiv papers on incremental indexing for code (memory-mapped index updates, embedding cache invalidation strategies, "small change → small reindex" heuristics) but no breakout systematic survey or canonical paper as of 2026-05 that's safe to assert by reference without verification. The practitioner community has converged on "watch the filesystem, debounce events, re-embed only changed files, and accept eventual consistency" — but this is folklore, not a citable result.

### 3.5 Practitioner posts on self-hosted code-index for AI agents

**Status** [emerging]: Through 2025 there were recurring blog series on dev.to, Medium, and personal blogs about "I built my own code-search MCP server because Cody is overkill and Cursor's index is opaque." Common motivations:
- Privacy (no code uploads)
- Cost (no per-seat pricing)
- Multi-IDE / multi-agent unification (one index, many consumers)
- Specific languages or domains underserved by commercial tools

The Fortemi use case in #736 is squarely in this niche.

---

## 4. Vendor MCP / API Patterns to Learn From

### 4.1 Continue.dev's Context Providers API

**Pattern** [established]: A `ContextProvider` exposes:
- `name`: stable identifier (`codebase`, `file`, `docs`, ...)
- `description`: shown in the `@`-completion UI
- `type`: `normal` (returns a list of items) or `submenu` (returns a tree)
- `getContextItems(query, options)`: async function returning `ContextItem[]`
- Optional `loadSubmenuItems`: for browsing

`ContextItem` contains `name`, `description`, `content`, `uri`. The agent receives content directly in-context.

**Lesson for Fortemi** [emerging]: A clean pluggable provider interface that returns `(metadata, content, uri)` triples is the right API shape for an MCP search tool. Fortemi's existing search response format should map naturally; the directory-archive case adds new metadata (relative path, file size, last modified) that the response shape should accommodate.

### 4.2 Cursor's `@`-mentions

**Pattern** [established]: User types `@`, IDE pops a fuzzy-search menu of available scopes (`@file`, `@folder`, `@codebase`, `@web`, `@docs`, plus per-language scopes). Selecting one expands into a `<scope>...</scope>` block in the chat input. Retrieval happens server-side.

**Lesson for Fortemi** [speculative]: The user-facing UX is the IDE's problem; what matters from a backend perspective is the protocol — Cursor's protocol is internal. If Fortemi wants to be addressable from arbitrary clients, MCP tool schemas (`semantic_search`, `list_files`, `read_chunk`) are the right surface.

### 4.3 GitHub Spark / Copilot Workspaces for external repos

**Status** [established]: GitHub Spark (the "vibe-coded apps" product, announced 2024) and Copilot Workspaces both work primarily against GitHub-hosted repos. Neither has a usable "point at a local directory not in GitHub" mode.

**Implication** [established]: This space is wide open for tools that don't require a git remote.

### 4.4 JetBrains AI Assistant local indexing model

**Pattern** [established]: JetBrains has decades-of-investment in incremental project indexing (PSI). Their AI Assistant rides on top of that — no separate "AI index." This is a fundamentally different architecture from the embedding-based approach: it uses the IDE's structural index instead of vector retrieval.

**Lesson for Fortemi** [emerging]: For projects already covered by tree-sitter, symbol-level retrieval without embeddings can be very effective and much cheaper to maintain (no embedding model, no vector store growth). Hybrid: structural retrieval for "find by name / call graph" + embedding retrieval for "find by semantic similarity" is the strongest combination. Fortemi already has this hybrid model for documents; extending to code with the same pattern is incremental.

---

## 5. Common Failure Modes Documented in 2024-2026

### 5.1 Secret leakage from auto-indexing

**Status** [established]: Multiple practitioner posts, GitHub issues, and at least one minor public incident through 2025 around code-indexing tools that:
- Indexed `.env` files because the user didn't gitignore them
- Embedded API keys into vector stores, then surfaced them in semantic-search results
- Persisted secrets in transitive caches even after the source file was redacted

**Examples** [emerging]:
- Continue.dev had issues filed around `.env` handling; their default ignore list grew over 2024-2025
- Community MCP servers commonly had no secrets handling at all in v1 releases
- At least one commercial code-search vendor publicly acknowledged a flaw in 2024 where customers' embeddings persisted secrets after rotation

**Mitigation patterns observed** [established]:
- Default-ignore for known secret-bearing patterns (`.env*`, `*.pem`, `*.key`, `id_rsa*`, `.aws/credentials`)
- Optional pre-ingest secret scanning (gitleaks, trufflehog as a hook)
- Re-ingest on rotation: don't try to "redact" embeddings; rebuild the chunk
- User-visible "what was indexed" diff before commit

**Application to Fortemi** [established]: This is the most important risk to address explicitly in Stream A's recommendations. The on-add scan must default to ignore-list-driven and surface what's about to be indexed before committing it.

### 5.2 Performance death on monorepos

**Status** [established]: Universally attested. The pattern is:
1. User points the tool at a 200k-file monorepo
2. Initial ingest takes hours and saturates disk I/O
3. Per-file embedding cost dominates (especially with cloud embedding APIs)
4. Resulting index is huge; queries are slow because the index doesn't fit in RAM
5. User gives up

**Mitigation patterns observed** [established]:
- Default per-extension ignores (anything in `node_modules`, `vendor`, `.git`, `dist`, `build`)
- File-size caps (skip files >1MB by default)
- Local embedding models (Ollama / nomic-embed-text) instead of OpenAI API for cost
- Progressive ingest: index the most-recently-touched 10% first, background the rest
- Chunking caps: don't embed minified or generated files

**Application to Fortemi** [established]: Fortemi already has nomic-embed-text as the default and document-type-aware ignores. The directory-archive feature must inherit these defaults with explicit, surfaced configuration.

### 5.3 Multi-tenant isolation failures

**Status** [emerging]: Less publicly documented because it requires multi-tenant deployment, which most personal-tool users don't have. But the failure mode is well-understood:
- Tool A's index leaks into Tool B's query results
- Embedding similarity collapses across tenant boundaries when chunks are too generic
- Filesystem permissions on the index store allow cross-tenant reads

**Fortemi's posture** [established]: The schema-per-archive model with `SET LOCAL search_path` (per CLAUDE.md) is far stronger than what any community MCP server I'm aware of provides. The directory-archive case must preserve this isolation: each referenced directory becomes a separate archive with a separate schema; queries cannot cross.

### 5.4 Sync drift between filesystem and index

**Status** [established]: The hardest problem in this space. Failure modes:
- File deleted on disk → embedding stays in index → semantic search surfaces a result that 404s on read
- File modified → old embedding still scored against query → stale chunk returned
- File renamed → old path indexed, new path not → query for new path misses
- File appears (e.g., git pull) → not indexed until next manual reingest → invisible to agent

**Mitigation patterns observed** [established]:
- inotify / fswatch on Linux+macOS, ReadDirectoryChangesW on Windows (the `notify` Rust crate Stream A already cites)
- Content-hash addressing (Stream A already cites BLAKE3) so a moved file is detected as the same content
- Periodic full reconciliation walk (cheap if hashes are cached) as a backstop for missed events
- Eventual-consistency semantics communicated to the agent ("results may be slightly stale")

**Application to Fortemi** [established]: Stream A's recommendations on `notify` + content-hash addressing directly address this. The remaining design choice is the reconciliation cadence (every query? every N minutes? on-demand only?).

---

## 6. Pricing / Cost Models for Local-Storage Code Indexing

### 6.1 Observed pricing dimensions [emerging]

| Model | Used by | Notes |
|---|---|---|
| Per-seat (flat) | Sourcegraph, Cursor, Continue paid tier | $20-$60/user/month typical; embeddings included |
| Per-embedding-token | None directly; passed through from OpenAI/Cohere | Cost-of-goods varies $0.02-$0.13/M tokens (2024-2025 prices); for a 1M-LOC codebase, initial embed is $5-$50 with cloud models |
| Per-line-of-code (annual) | Some enterprise code-search vendors (legacy) | Falling out of favor; hard to forecast |
| Self-hosted, BYO-embedding-model | Fortemi (current posture), Continue with Ollama | Operator cost is hardware + electricity; nominally free |
| Storage-byte pricing | None directly; transit cost on cloud-hosted | Vector embeddings: ~6KB per chunk at 1536d float32, ~1.5KB at int8 |

### 6.2 Implications for Fortemi resource conversations [emerging]

The relevant dimension for Fortemi's Construction-phase planning is **operator-facing cost transparency**, not pricing-to-end-users (since Fortemi is self-hosted):

- "How much disk does indexing a 100MB code directory consume?" — answer depends on chunking granularity and embedding dimensions; ballpark 200-400MB index for 100MB of source with default settings
- "How much GPU time for initial ingest?" — depends on embedding model; nomic-embed-text on a 3060 12GB runs ~500 chunks/sec, so 50k files (~500k chunks) is ~17 minutes
- "How much for incremental updates?" — typically <1% of initial cost if hash-cached

These are the numbers the operator needs to set MAX_MEMORIES budgets sensibly (per CLAUDE.md's existing per-VRAM guidance).

---

## 7. Open Questions for Synthesis (Phase 2)

The synthesizer must resolve or explicitly defer these:

1. **Should the directory-archive be a new archive type or a new storage mode on the existing archive type?** (Stream A leans toward the latter via `StorageMode` enum; this affects schema, API, and the existing `manage_archives` MCP tool surface.)

2. **What's the live-update model — push (fswatch), pull (periodic walk), or query-time check?** Each has trade-offs; the answer affects the resource budget more than any other single choice.

3. **For monorepos, what's the default behavior — index everything with sensible defaults, or require the user to scope explicitly via an include/exclude config?** Other tools default opposite ways; both have failure modes.

4. **Should symbol-level (tree-sitter) and chunk-level retrieval be both available, or pick one?** Existing Fortemi document-type chunking has a precedent; extending vs replacing matters for the schema migration.

5. **What's the secret-leakage default posture — opt-in scanning (cheaper, less safe) or opt-out (slower initial ingest, safer)?** The recent industry pattern is opt-out; this conflicts with "fast initial ingest" goals.

6. **How does the directory-archive interact with the existing federated-search API?** A user with three directory-archives plus their default support archive should plausibly be able to federated-search across all four; the schema implications need confirming.

7. **What's the MCP tool surface — extend `manage_archives` and the existing search tools, or introduce a new `directory_archive_*` family?** Backward compatibility vs cleanliness.

8. **Resource isolation: should each directory-archive get its own PostgreSQL schema (per CLAUDE.md's existing pattern) or share with the user's default archive?** Per-schema is consistent but adds operational weight; shared is simpler but breaks the isolation invariant.

9. **What happens on directory disappearance (mount unmounts, drive ejects)?** Read errors → graceful degradation? Mark archive offline? The current Fortemi error handling assumes always-available storage.

10. **Is there a meaningful "Fortemi as an MCP source for Claude Code / Cline / Continue" product story, or is this a Fortemi-the-app feature?** This affects whether the MCP surface should be designed for first-party use only or for third-party consumption, which has API-stability implications.

---

## References (cross-stream)

- @.aiwg/working/issue-planner-storage/research-best-practices.md — Stream A patterns this stream complements
- @.aiwg/research/findings/REF-089-recursive-language-models.md — Lossless retrieval as the alternative to compaction (cited above)
- @CLAUDE.md — Fortemi current architecture, including multi-memory schema model and MCP server topology
- Fortemi MCP server at `mcp-server/` (port 3001) — existing surface that the directory-archive feature would extend
