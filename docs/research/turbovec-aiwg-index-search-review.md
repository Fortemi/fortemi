# TurboVec and AIWG Index Search Review

Date: 2026-05-22

## Source

- Repository: https://github.com/RyanCodrai/turbovec
- License: MIT
- Local inspection clone: `/tmp/turbovec`
- Upstream release context observed: Python package 0.5.2 and Rust crate 0.5.0 dated 2026-05-21 in `CHANGELOG.md`

## Executive Summary

TurboVec is not a replacement architecture for Fortemi or AIWG search/discovery. It is a compact, MIT-licensed implementation of train-free compressed vector search with several ideas worth tracking as enhancements:

- train-free quantized vector indexing for append-heavy corpora;
- mask/allowlist-aware search as a first-class primitive;
- stable external IDs layered over positional vector storage;
- explicit file format versioning when scoring semantics change;
- lazy search caches with an explicit `prepare()` warm-up path;
- thin framework adapters that keep integration logic outside the hot search path.

The most relevant gap for AIWG is not "use TurboVec directly." AIWG already has an artifact metadata index, graph traversal, and an optional HNSW embedding layer. The gap is that capability discovery and artifact search still rely primarily on lexical/token scoring, while the semantic embedding layer is optional, lightly surfaced, and lacks filtered top-k semantics that compose cleanly with graph/type/provider constraints.

## TurboVec Concepts Worth Considering

### Train-Free Compressed Search

TurboVec normalizes vectors, applies a deterministic orthogonal rotation, quantizes rotated coordinates using Lloyd-Max centroids derived from the expected coordinate distribution, and searches directly over bit-packed codes. This avoids training a corpus-specific product quantizer and fits append-heavy usage.

Fortemi already uses pgvector/HNSW for durable application search. TurboVec is more interesting as an optional local/agent-side acceleration layer or compressed cache than as a primary database index replacement.

### Mask and Allowlist Semantics

TurboVec exposes filtered search in two layers:

- positional `search_with_mask`;
- external-ID `IdMapIndex.search_with_allowlist`.

The important design point is semantic: the top-k result set is computed within the allowed subset, not searched globally and filtered after the fact. The SIMD path can skip whole 32-vector blocks when a mask excludes a block.

This maps well to AIWG/Fortemi constraints:

- tenant and workspace boundaries;
- provider-specific capability surfaces;
- artifact type filters such as skill/agent/command/rule;
- graph-specific scopes such as framework/project/codebase/kb;
- search within an RLM candidate set.

### Stable ID Wrapper

TurboVec keeps the fast storage positional and wraps it with `IdMapIndex` for stable external IDs. This is a useful pattern for any local vector cache: store compact arrays for speed, and maintain a small bidirectional ID table for artifact paths or capability IDs.

### Versioned Persistence

TurboVec rejects old index files when a stored scalar changed meaning. This is the right failure mode for retrieval indexes: stale indexes should fail clearly rather than silently return skewed scores.

AIWG already has index format version fields. The lesson is to apply the same strictness to optional semantic/vector sidecars if their embedding model, dimensionality, quantization, scoring, or source text changes.

## Current AIWG Search/Discovery Shape

Relevant AIWG components inspected:

- `src/artifacts/query-engine.ts`
- `src/artifacts/index-builder.ts`
- `src/artifacts/types.ts`
- `src/artifacts/embedding-index.ts`
- `src/artifacts/hybrid-query.ts`
- `src/artifacts/graph-query.ts`
- `agentic/code/frameworks/sdlc-complete/rules/artifact-discovery.md`

The current AIWG index provides:

- metadata extraction for artifacts, skills, agents, commands, rules, docs, and project files;
- lexical scoring over name, triggers, capability, title, tags, summary, path, and type;
- graph-separated indexes for framework, project, and codebase;
- dependency/neighbor traversal over typed edges;
- optional semantic embeddings using `@xenova/transformers` plus `hnswlib-node`;
- checksum-based incremental rebuild support for metadata;
- JSON-first CLI output for agent consumption.

Observed local Fortemi project index stats from `aiwg index stats --json`:

- 14 indexed project artifacts;
- 99 total `.aiwg` files;
- 14% project index coverage;
- 0 dependency edges.

That means the project-local artifact index is currently useful as inventory, but not yet a rich dependency or semantic graph for this workspace.

## Gap Analysis

### Already Covered

- Metadata index and structured JSON query output.
- Multi-graph separation: framework, project, codebase, and user-defined graph support.
- Dependency and neighbor traversal.
- Optional HNSW semantic index module exists.
- Index format versioning exists at metadata level.

### Partially Covered

- Semantic retrieval: `embedding-index.ts` can build/query HNSW vectors, but discovery currently ranks through `scoreEntry()` lexical heuristics.
- Hybrid query: `hybrid-query.ts` names semantic search, but currently implements keyword scoring rather than embedding search.
- Incremental detection: embedding change detection exists, but integration with `index build`, stale handling, and CLI query paths needs audit.
- Filtering: lexical query applies filters before ranking. The embedding query path does not appear to expose equivalent allowlist/top-k-within-filter semantics.

### Missing or Worth Auditing

- A documented query path that fuses lexical scores, semantic scores, and graph/type filters.
- A filtered semantic query contract: top-k among allowed graph/type/tag/provider candidates.
- Persistent semantic sidecar version checks keyed by model, dimensions, source text recipe, and index/scoring version.
- Stable ID mapping documentation for vector sidecars, where IDs are artifact paths/capability names and vector positions are implementation details.
- Discovery telemetry for why a result ranked: lexical trigger, semantic neighbor, graph proximity, or exact-name floor.
- Coverage guidance for project graphs, since this workspace currently indexes only 14 of 99 `.aiwg` files and has no dependency edges.

## Recommendations

1. Audit AIWG's optional embedding index path end to end before adding new dependencies or replacing HNSW.
2. Define a filtered semantic query contract: candidate set first, vector top-k second, no post-filtered global top-k surprises.
3. Consider a compressed local vector sidecar only if agent/workspace-scale indexes become large enough that HNSW storage or startup cost matters.
4. Add strict semantic sidecar compatibility checks: model ID, dims, text recipe version, embedding backend, index backend, and scoring version.
5. Improve project index coverage for Fortemi before expecting AIWG index discovery to answer project-local questions reliably.
6. Keep TurboVec as an enhancement reference, not a primary stack migration.

## Follow-Up Audit Questions

- Is `embedding-index.ts` wired into `aiwg index build`, `query`, `discover`, or only available as a library module?
- Should `aiwg discover` optionally use semantic retrieval after lexical exact-name and trigger matching?
- Should graph/type/tag filters be compiled into an allowlist for semantic search?
- What is the expected rebuild behavior when embedding model configuration changes?
- Would a compressed vector cache improve any real AIWG corpus size today, or is HNSW sufficient?
- Should Fortemi use a similar mask-aware contract for tenant/search-scope filtering around semantic search?
