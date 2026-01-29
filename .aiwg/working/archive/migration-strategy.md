# Memory Migration Strategy

**Status**: Active
**Created**: 2026-01-02
**Updated**: 2026-01-02

## Overview

This document tracks the migration of the memory/embedding subsystem from HotM to matric-memory as an independent library crate.

## Source Analysis Summary

**HotM Server Location**: `/home/roctinam/dev/hotm/server/`
**Total Production Code**: ~7,000 lines (215 KB)

### Key Components to Migrate

| Component | Source File(s) | Lines | Priority | Target Crate |
|-----------|---------------|-------|----------|--------------|
| Database Layer | db.rs, db_enhanced_v2.rs | 1,800 | Critical | matric-db |
| Models | models.rs | 1,053 | Critical | matric-core |
| Search Engine | routes/search.rs | 501 | High | matric-search |
| Inference | ollama.rs | 527 | High | matric-inference |
| Job Queue | job_queue.rs | 842 | High | matric-jobs |
| WebSocket | websocket.rs | 180 | Medium | matric-core (traits) |
| Routes | routes/*.rs | 800+ | N/A | Stays in HotM |

### Dependencies (from HotM Cargo.toml)

**Core (migrate to matric-memory)**:
- `sqlx 0.8.6` (postgres, runtime-tokio-native-tls, macros, migrate)
- `pgvector 0.4.1`
- `tokio 1` (full)
- `serde 1`, `serde_json 1`
- `reqwest 0.12` (json)
- `uuid 1`, `chrono 0.4`
- `thiserror 1`, `anyhow 1`
- `tracing 0.1`

**Application-specific (stays in HotM)**:
- `axum 0.7` (ws)
- `tower-http 0.5` (cors)
- `tokio-stream 0.1`

## Migration Phases

### Phase 1: Workspace Setup (Issue #6)

**Goal**: Create Cargo workspace structure

```
matric-memory/
├── Cargo.toml                 # Workspace root
├── crates/
│   ├── matric-core/           # Core traits, types, errors
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── matric-db/             # PostgreSQL + pgvector layer
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── matric-search/         # FTS + semantic + hybrid search
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   ├── matric-inference/      # LLM backend abstraction
│   │   ├── Cargo.toml
│   │   └── src/lib.rs
│   └── matric-jobs/           # Job queue system
│       ├── Cargo.toml
│       └── src/lib.rs
├── migrations/                # Database migrations
└── examples/                  # Usage examples
```

**Tasks**:
1. Create workspace Cargo.toml
2. Create each crate with minimal structure
3. Set up inter-crate dependencies
4. Verify workspace builds

### Phase 2: Core Types Migration (Issue #2)

**Goal**: Extract core types and traits to matric-core

**From models.rs**:
- `NoteMeta`, `NoteOriginal`, `NoteRevised`, `NoteFull`
- `Link`, `SearchHit`, `SearchResponse`
- `JobStatus`, `JobType`, `Job`
- Error types

**New abstractions**:
- `InferenceBackend` trait
- `SearchProvider` trait
- `JobProcessor` trait

### Phase 3: Database Layer (Issue #3)

**Goal**: Migrate database operations to matric-db

**Components**:
- Connection pool setup
- CRUD operations for notes, revisions, embeddings
- Schema migrations (copy from HotM)
- SQLx compile-time verification

**Strategy**:
1. Copy migrations from HotM
2. Extract db.rs operations as library functions
3. Keep AppState pattern but make configurable
4. Add feature flags for optional features

### Phase 4: Search Engine (Issue #4)

**Goal**: Implement hybrid search in matric-search

**Components**:
- FTS (tsvector/GIN)
- Semantic (pgvector similarity)
- Hybrid (RRF fusion)
- Related notes discovery

**Strategy**:
1. Extract search functions from routes/search.rs
2. Make model-agnostic (accept trait objects)
3. Expose SearchConfig for tuning weights

### Phase 5: Inference Backend (Issue #7-11)

**Goal**: Create pluggable inference abstraction

**Components**:
- InferenceBackend trait (#8)
- Ollama implementation (#9)
- OpenAI-compatible implementation (#10)
- Configuration & routing (#11)

**Strategy**:
1. Extract ollama.rs logic
2. Define trait with embed_texts() and generate() methods
3. Implement Ollama backend
4. Add OpenAI backend

### Phase 6: Job Queue (Issue #5)

**Goal**: Migrate job queue to matric-jobs

**Components**:
- Job types and status
- Queue management
- Job processing
- Progress tracking

**Strategy**:
1. Extract job_queue.rs logic
2. Make job processor injectable
3. Add JobNotifier trait for status updates

## HotM Update Strategy

After matric-memory is functional, update HotM to consume it:

### Step 1: Add Dependency

```toml
# HotM server/Cargo.toml
[dependencies]
matric-memory = { path = "../../matric-memory" }
# Or: matric-memory = { git = "https://git.integrolabs.net/roctinam/matric-memory" }
```

### Step 2: Replace Imports

```rust
// Before (in HotM)
use crate::db::*;
use crate::models::*;
use crate::ollama::*;
use crate::job_queue::*;

// After
use matric_core::*;
use matric_db::*;
use matric_search::*;
use matric_inference::*;
use matric_jobs::*;
```

### Step 3: Thin Wrapper Routes

HotM routes become thin wrappers:
- Request parsing (Axum)
- Call matric-memory functions
- Response formatting
- Error mapping

### Step 4: WebSocket Integration

HotM keeps WebSocket layer but receives events from matric-jobs via callback/channel.

## Testing Strategy

### Unit Tests (per crate)
- matric-core: Type serialization, error handling
- matric-db: SQL query construction, mock pool
- matric-search: RRF algorithm, scoring
- matric-inference: Request/response parsing
- matric-jobs: Queue operations, status transitions

### Integration Tests
- Full pipeline with test PostgreSQL
- Search quality validation
- Embedding verification

### End-to-End Tests
- HotM consuming matric-memory
- Full note lifecycle
- Search accuracy

## Risk Mitigation

### Risk: Breaking HotM During Migration
**Mitigation**:
- Feature branches in both repos
- Parallel code (don't delete HotM originals until validated)
- CI/CD runs both test suites

### Risk: API Boundary Instability
**Mitigation**:
- Start with minimal public API
- Use feature flags for experimental APIs
- Comprehensive rustdoc with examples

### Risk: pgvector Performance
**Mitigation**:
- Benchmark before/after migration
- Document performance characteristics
- Have Qdrant fallback ready

## Current Status

- [x] Source analysis complete
- [ ] Workspace structure (#6)
- [ ] Public API definition (#2)
- [ ] Database layer (#3)
- [ ] Search engine (#4)
- [ ] Job queue (#5)
- [ ] CI/CD (#20)
- [ ] Documentation (#17-19)
