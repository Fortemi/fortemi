# Use Cases and User Stories: Graph Topology Improvement (Issue #386)

**Date**: 2026-02-14
**Status**: Draft
**Epic**: Graph Topology Enhancement
**Related Research**: `docs/research/graph-topology-executive-summary.md`

---

## Executive Summary

This document defines use cases and user stories for implementing mutual k-Nearest Neighbors (k-NN) linking strategy in Fortemi. The current threshold-based linking creates star topologies (dense clusters around hub notes) that limit multi-hop graph traversal. The k-NN solution creates mesh-of-stars topology with distributed connections, enabling richer knowledge discovery through graph exploration.

**Key Changes**:
- Add configurable linking strategy (threshold vs. mutual k-NN)
- Implement adaptive k-value based on corpus size
- Add fallback logic for isolated nodes
- Expose graph topology metrics via new API endpoint
- Maintain backward compatibility with existing links

---

## Use Cases

### UC-001: Agent Discovers Related Knowledge via Multi-Hop Traversal

**Actor**: AI Agent (via MCP `explore_graph` tool)

**Preconditions**:
- Knowledge base contains >= 100 notes across multiple topics
- Notes have been linked using mutual k-NN strategy
- Agent has access to MCP server with graph exploration tools

**Main Success Scenario**:
1. Agent calls `explore_graph` with `note_id=A`, `depth=3`, `max_results=20`
2. System performs breadth-first traversal using semantic links
3. System returns graph containing:
   - Direct neighbors (depth 1): 7 notes closely related to A
   - Second-degree neighbors (depth 2): 15 notes bridging to adjacent topics
   - Third-degree neighbors (depth 3): 12 notes in related domains
4. Agent discovers note B at depth 3 that shares conceptual overlap but different terminology
5. Agent synthesizes insights connecting concepts from notes A and B

**Postconditions**:
- Graph contains paths of length > 1 between topically related notes
- Agent successfully navigates across topic boundaries
- Traversal completes within 500ms for depth <= 3

**Alternate Flows**:

**AF-001a: Sparse Graph Returns Limited Results**
- At step 3, graph contains < 20 total notes (small corpus or isolated note)
- System returns all reachable notes within depth limit
- Agent receives warning in response metadata: `"graph_density": "sparse"`

**AF-001b: Depth Limit Prevents Discovery**
- At step 4, note B exists at depth 4 (beyond requested depth 3)
- System does not return note B
- Agent can retry with increased depth limit

**Exception Flows**:

**EF-001a: Note Has No Links (Isolated Node)**
- At step 2, note A has zero semantic links
- System returns empty graph with single node (note A itself)
- Error metadata: `"isolation_reason": "no_mutual_neighbors"`

**EF-001b: Database Connection Failure**
- At step 2, database query times out
- System returns HTTP 504 Gateway Timeout
- Agent can retry with exponential backoff

---

### UC-002: Admin Configures Linking Strategy

**Actor**: System Administrator

**Preconditions**:
- Admin has shell access to Fortemi deployment
- System is running in Docker bundle or development environment
- Admin has read documentation on linking strategies

**Main Success Scenario**:
1. Admin edits `.env` file and sets:
   ```
   GRAPH_LINKING_STRATEGY=mutual_knn
   GRAPH_K_NEIGHBORS=7
   ```
2. Admin restarts Fortemi services:
   ```bash
   docker compose -f docker-compose.bundle.yml down
   docker compose -f docker-compose.bundle.yml up -d
   ```
3. System loads configuration on startup
4. System logs confirmation:
   ```
   [INFO] Graph linking strategy: mutual_knn (k=7)
   ```
5. Admin creates new note via API
6. System triggers linking job with mutual k-NN strategy
7. New note is linked to its 7 nearest neighbors (if mutual)
8. Existing notes retain their previous links (no automatic re-linking)

**Postconditions**:
- All new notes use mutual k-NN linking strategy
- Existing notes preserve threshold-based links
- System configuration is persisted across restarts

**Alternate Flows**:

**AF-002a: Admin Reverts to Threshold Strategy**
- At step 1, admin sets `GRAPH_LINKING_STRATEGY=threshold`
- At step 6, system uses threshold-based linking (original behavior)
- Links are created bidirectionally to all notes exceeding similarity threshold

**AF-002b: Admin Sets Invalid k Value**
- At step 1, admin sets `GRAPH_K_NEIGHBORS=invalid`
- At step 3, system fails to parse configuration
- System logs error and falls back to default (k=7)
- System continues startup with default configuration

**Exception Flows**:

**EF-002a: Missing Required Configuration**
- At step 3, `GRAPH_LINKING_STRATEGY` is set but env var is malformed
- System logs warning: `Invalid GRAPH_LINKING_STRATEGY, using default: threshold`
- System falls back to threshold-based linking

---

### UC-003: System Adapts k to Corpus Size

**Actor**: System (automatic behavior during linking job)

**Preconditions**:
- `GRAPH_LINKING_STRATEGY=mutual_knn` (or auto)
- Admin has NOT explicitly set `GRAPH_K_NEIGHBORS` (env var is unset)
- Knowledge base contains notes ranging from 10 to 10,000 count

**Main Success Scenario**:
1. Linking job begins for note A
2. System queries total note count: `SELECT COUNT(*) FROM notes`
3. System computes adaptive k value:
   ```
   k = CLAMP(log₂(N), 5, 15)
   ```
   where N = total note count
4. System uses computed k for mutual k-NN candidate selection
5. System logs: `Adaptive k={computed_k} for corpus size N={total_notes}`

**Examples**:
| Corpus Size (N) | log₂(N) | Clamped k | Rationale |
|-----------------|---------|-----------|-----------|
| 10 notes | 3.32 | 5 | Minimum k prevents isolation |
| 100 notes | 6.64 | 7 | Small corpus, moderate connectivity |
| 1,000 notes | 9.97 | 10 | Medium corpus, balanced |
| 10,000 notes | 13.29 | 13 | Large corpus, higher connectivity |
| 100,000 notes | 16.61 | 15 | Maximum k prevents over-connection |

**Postconditions**:
- k value scales logarithmically with corpus growth
- Small corpora (< 32 notes) use minimum k=5
- Large corpora (> 32,768 notes) use maximum k=15
- Behavior is deterministic for given corpus size

**Alternate Flows**:

**AF-003a: Admin Overrides with Explicit k**
- Precondition change: `GRAPH_K_NEIGHBORS=10` is set in environment
- At step 3, system skips adaptive calculation
- System uses fixed k=10 regardless of corpus size

**Exception Flows**:

**EF-003a: Corpus Size Query Fails**
- At step 2, database query returns error
- System falls back to default k=7
- System logs warning: `Failed to compute adaptive k, using default`

---

### UC-004: Isolated Node Fallback

**Actor**: System (automatic behavior during linking job)

**Preconditions**:
- `GRAPH_LINKING_STRATEGY=mutual_knn`
- Note A has unique content not similar to any other notes
- Corpus contains >= 10 other notes

**Main Success Scenario**:
1. Linking job begins for note A
2. System computes k-NN candidates (e.g., k=7)
3. System finds candidates: `[{note_id: B, score: 0.42}, {note_id: C, score: 0.38}, ...]`
4. System checks mutuality for each candidate:
   - Note B's k-NN does NOT include A (not mutual)
   - Note C's k-NN does NOT include A (not mutual)
   - All candidates fail mutuality check
5. Mutual k-NN returns empty result set
6. System detects isolation condition: `mutual_links.len() == 0 && candidates.len() > 0`
7. System applies fallback rule: Create single link to best match (highest score)
8. System creates link: `note_id=A -> note_id=B, type=semantic, score=0.42, metadata={"fallback": true}`
9. System logs: `Isolated node fallback: created 1 link (best match)`

**Postconditions**:
- Note A has exactly 1 outgoing link (to best match)
- Link is marked as fallback in metadata
- Note is not completely isolated from graph
- Graph remains weakly connected (no unreachable components)

**Alternate Flows**:

**AF-004a: Mutual k-NN Finds Partial Matches**
- At step 5, mutual k-NN returns 3 links (< k, but > 0)
- System creates 3 mutual links
- System does NOT apply fallback (partial success is acceptable)
- System logs: `Created 3 mutual links (partial k-NN)`

**Exception Flows**:

**EF-004a: Zero Candidates (First Note in Corpus)**
- At step 3, k-NN search returns empty (no other notes exist)
- At step 6, condition is `mutual_links.len() == 0 && candidates.len() == 0`
- System skips fallback (no links possible)
- System logs: `No candidates for linking (corpus size = 1)`

**EF-004b: All Candidates Are Self-Links**
- At step 3, all k-NN candidates resolve to note A itself
- System filters self-links: `candidates.retain(|c| c.note_id != note_id)`
- Filtered candidates list becomes empty
- Treated as EF-004a (no fallback link created)

---

### UC-005: Monitor Graph Health Metrics

**Actor**: System Administrator or AI Agent

**Preconditions**:
- Fortemi API is running and accessible
- Knowledge base contains >= 10 notes with semantic links
- Actor has appropriate authentication credentials (if `REQUIRE_AUTH=true`)

**Main Success Scenario**:
1. Actor sends request:
   ```
   GET /api/v1/graph/topology/stats
   Authorization: Bearer {token}
   ```
2. System queries graph structure:
   - Count total notes and links
   - Compute degree distribution (links per note)
   - Identify triangles for clustering coefficient
   - Sample random node pairs for average path length
3. System computes metrics:
   - `clustering_coefficient = (3 × triangles) / connected_triples`
   - `avg_degree = (2 × edges) / nodes`
   - `degree_std_dev = std_dev(node_degrees)`
   - `avg_path_length = avg(sampled_shortest_paths)`
4. System classifies topology type:
   - If `avg_degree_std_dev > 5 && clustering < 0.1`: "star"
   - If `clustering >= 0.3 && avg_degree_std_dev < 3`: "mesh"
   - Else: "mixed"
5. System returns JSON response:
   ```json
   {
     "total_notes": 1247,
     "total_links": 8729,
     "avg_degree": 7.0,
     "degree_std_dev": 2.1,
     "clustering_coefficient": 0.42,
     "avg_path_length": 3.2,
     "topology_type": "mesh",
     "sample_size": 100,
     "computed_at": "2026-02-14T15:30:00Z"
   }
   ```

**Postconditions**:
- Actor receives quantitative graph health metrics
- Metrics enable comparison before/after linking strategy changes
- Data is suitable for time-series monitoring and alerting

**Alternate Flows**:

**AF-005a: Large Graph Requires Sampling**
- At step 2, graph contains > 10,000 notes
- System switches to sampling mode:
  - Degree distribution: full calculation (cheap)
  - Clustering coefficient: sample 1,000 random nodes
  - Path length: sample 500 random node pairs
- Response includes `"sampled": true` flag
- Metrics are approximations with ~95% confidence

**AF-005b: Very Small Graph**
- At step 2, graph contains < 10 notes
- Clustering coefficient and path length become unstable
- System returns metrics with warning:
  ```json
  {
    "warning": "Graph too small for reliable metrics (N < 10)",
    "total_notes": 7
  }
  ```

**Exception Flows**:

**EF-005a: No Links Exist (Disconnected Graph)**
- At step 2, total link count is 0
- System cannot compute clustering or path length
- System returns:
  ```json
  {
    "total_notes": 50,
    "total_links": 0,
    "topology_type": "disconnected",
    "error": "No links exist in graph"
  }
  ```

**EF-005b: Database Query Timeout**
- At step 2, complex graph query exceeds 30-second timeout
- System returns HTTP 504 Gateway Timeout
- Response includes partial results if available:
  ```json
  {
    "error": "Query timeout during path length calculation",
    "partial_results": {
      "total_notes": 15432,
      "avg_degree": 8.2
    }
  }
  ```

---

### UC-006: Compare Topology Before and After Strategy Change

**Actor**: System Administrator or Data Analyst

**Preconditions**:
- System is currently using threshold-based linking
- Admin has collected baseline metrics via GET `/api/v1/graph/topology/stats`
- Knowledge base contains >= 100 notes for statistical validity

**Main Success Scenario**:
1. Admin records baseline metrics:
   ```json
   {
     "strategy": "threshold",
     "avg_degree": 12.3,
     "degree_std_dev": 8.7,
     "clustering_coefficient": 0.05,
     "avg_path_length": 2.1,
     "topology_type": "star"
   }
   ```
2. Admin changes configuration to `GRAPH_LINKING_STRATEGY=mutual_knn`
3. Admin triggers bulk re-linking job:
   ```
   POST /api/v1/graph/relink
   {
     "strategy": "mutual_knn",
     "k": 7,
     "apply_to_existing": true
   }
   ```
4. System processes all notes with new linking strategy
5. System clears old semantic links and creates new mutual k-NN links
6. Job completes with summary: `{links_removed: 15432, links_created: 8729}`
7. Admin queries topology metrics again
8. Admin compares results:
   ```json
   {
     "strategy": "mutual_knn",
     "avg_degree": 7.0,
     "degree_std_dev": 2.1,
     "clustering_coefficient": 0.42,
     "avg_path_length": 3.2,
     "topology_type": "mesh"
   }
   ```
9. Admin validates improvement:
   - Clustering coefficient increased from 0.05 to 0.42 (8.4x improvement)
   - Degree distribution became more uniform (std_dev reduced 4.1x)
   - Topology shifted from "star" to "mesh"

**Postconditions**:
- Graph topology reflects mutual k-NN structure
- Quantitative metrics confirm mesh-of-stars formation
- Admin has data to validate strategy change

**Alternate Flows**:

**AF-006a: Partial Re-linking (New Notes Only)**
- At step 3, admin does NOT set `apply_to_existing: true`
- System only applies k-NN to notes created after configuration change
- Graph becomes hybrid: old notes use threshold, new notes use k-NN
- Topology metrics reflect mixed strategy

**AF-006b: Metrics Show Degradation**
- At step 9, admin discovers `avg_path_length` increased beyond acceptable threshold
- Admin decides to revert: set `GRAPH_LINKING_STRATEGY=threshold`
- Admin triggers re-linking job to restore original topology
- System rolls back to baseline metrics

---

### UC-007: Agent Queries Graph Traversal Depth Distribution

**Actor**: AI Agent or Analytics System

**Preconditions**:
- System is logging link click events with depth metadata
- Minimum 1,000 link traversal events have been recorded
- Analytics endpoint is accessible

**Main Success Scenario**:
1. Agent queries traversal analytics:
   ```
   GET /api/v1/analytics/graph/traversal-depth
   ```
2. System aggregates link click events by depth:
   ```sql
   SELECT depth, COUNT(*) as clicks
   FROM link_click_events
   WHERE timestamp > NOW() - INTERVAL '30 days'
   GROUP BY depth
   ORDER BY depth;
   ```
3. System computes depth distribution:
   ```json
   {
     "depth_distribution": {
       "1": 7234,
       "2": 2156,
       "3": 543,
       "4": 87,
       "5+": 12
     },
     "pct_depth_gt_1": 27.8,
     "avg_depth": 1.4,
     "max_depth_observed": 7
   }
   ```
4. Agent compares against baseline (threshold strategy):
   - Baseline `pct_depth_gt_1`: 8.2%
   - Current `pct_depth_gt_1`: 27.8%
   - Improvement: 3.4x increase in multi-hop navigation

**Postconditions**:
- Agent has quantitative data on user engagement with graph traversal
- Metrics validate that mesh topology enables deeper exploration
- Data can be visualized in monitoring dashboard

**Alternate Flows**:

**AF-007a: Insufficient Data**
- At step 2, query returns < 100 events
- System returns warning: `"insufficient_data": "< 100 events in time window"`
- Agent retries with longer time window or waits for more data

---

### UC-008: Handle k-NN Computation Failure

**Actor**: System (during linking job execution)

**Preconditions**:
- Linking job is triggered for note A
- `GRAPH_LINKING_STRATEGY=mutual_knn`
- Database or embedding service encounters error

**Main Success Scenario** (Graceful Degradation):
1. System queries k-NN candidates via pgvector:
   ```sql
   SELECT note_id, 1 - (vector <=> ?) as score
   FROM embeddings
   ORDER BY vector <=> ?
   LIMIT ?
   ```
2. Database connection times out or returns error
3. System catches exception and logs:
   ```
   [ERROR] k-NN query failed: database timeout (30s exceeded)
   ```
4. System falls back to threshold-based linking:
   - Queries similar notes with threshold filter
   - Creates bidirectional links to all notes > threshold
5. System marks job as partial success:
   ```json
   {
     "status": "success_with_warnings",
     "links_created": 12,
     "warnings": ["k-NN computation failed, used threshold fallback"]
   }
   ```

**Postconditions**:
- Note has links created via fallback strategy
- Job does not fail completely
- Error is logged for admin investigation

**Exception Flows**:

**EF-008a: Both k-NN and Fallback Fail**
- At step 4, threshold-based query also fails
- System marks job as failed
- No links are created
- System returns:
   ```json
   {
     "status": "failed",
     "error": "Both k-NN and threshold linking failed",
     "retry_recommended": true
   }
   ```

---

## User Stories

### Foundational Stories (Configuration and Setup)

#### US-001: Configure Linking Strategy via Environment Variable

**As a** system administrator
**I want to** set the graph linking strategy via environment variable
**So that** I can control topology behavior without code changes

**Acceptance Criteria**:
- Given the system is deployed with default configuration
- When I set `GRAPH_LINKING_STRATEGY=mutual_knn` in `.env`
- And restart Fortemi services
- Then all subsequent linking jobs use mutual k-NN algorithm
- And existing links are preserved (no automatic re-linking)
- And system logs confirm: `Graph linking strategy: mutual_knn`

**Edge Cases**:
- Invalid strategy name falls back to default (threshold)
- Missing env var uses default strategy
- Case-insensitive parsing (MUTUAL_KNN == mutual_knn)

**Test Data**:
```bash
# .env
GRAPH_LINKING_STRATEGY=mutual_knn
GRAPH_K_NEIGHBORS=7
```

---

#### US-002: Adaptive k Scales with Corpus Size

**As a** Fortemi instance owner
**I want** the system to automatically adjust k based on corpus size
**So that** small knowledge bases don't over-connect and large ones don't under-connect

**Acceptance Criteria**:
- Given `GRAPH_K_NEIGHBORS` is NOT set in environment
- When a linking job runs for any note
- Then system computes `k = CLAMP(log₂(N), 5, 15)` where N = total note count
- And uses computed k for mutual k-NN candidate selection
- And logs the adaptive k value: `Adaptive k=7 for corpus size N=100`

**Edge Cases**:
- Corpus with 1 note: k=5 (minimum)
- Corpus with 100,000 notes: k=15 (maximum)
- Database query failure falls back to default k=7

**Test Cases**:
| Corpus Size | Expected k |
|-------------|------------|
| 10 | 5 |
| 100 | 7 |
| 1,000 | 10 |
| 10,000 | 13 |

---

#### US-003: Explicit k Override Takes Precedence

**As a** system administrator
**I want to** override adaptive k with a fixed value
**So that** I can manually tune linking density for specific use cases

**Acceptance Criteria**:
- Given `GRAPH_K_NEIGHBORS=10` is set in `.env`
- When a linking job runs with corpus size = 100 (which would compute k=7)
- Then system uses k=10 instead of adaptive value
- And logs: `Using explicit k=10 (overrides adaptive k=7)`

**Edge Cases**:
- k=0 is rejected, falls back to adaptive
- k > 50 triggers warning but is allowed
- Non-integer values are rounded down

---

### Linking Behavior Stories

#### US-004: Mutual k-NN Creates Bidirectional Links

**As a** knowledge graph user
**I want** semantic links to be mutually recognized
**So that** link quality is higher (both notes agree on relationship)

**Acceptance Criteria**:
- Given note A has embedding vector V_A
- And note B is in A's 7-NN list
- And note A is in B's 7-NN list (mutuality confirmed)
- When linking job runs for note A
- Then system creates bidirectional link: A ↔ B
- And link metadata includes: `{"mutual_knn": true, "k": 7, "score": 0.82}`

**Edge Cases**:
- A is in B's k-NN but B is NOT in A's k-NN: no link created
- Self-links (A in its own k-NN) are filtered out
- Duplicate link creation is silently ignored (UNIQUE constraint)

---

#### US-005: Isolated Node Gets Fallback Link

**As a** knowledge graph maintainer
**I want** every note to have at least one link
**So that** the graph remains weakly connected (no unreachable components)

**Acceptance Criteria**:
- Given note A is very dissimilar to all other notes
- And mutual k-NN returns zero mutual neighbors
- And k-NN candidates list is non-empty (corpus size > 1)
- When linking job completes for note A
- Then system creates single link to best match (highest similarity score)
- And link metadata includes: `{"fallback": true, "reason": "no_mutual_neighbors"}`
- And system logs: `Isolated node fallback: created 1 link`

**Edge Cases**:
- First note in corpus (zero candidates): no fallback link
- All candidates are below absolute minimum threshold (e.g., score < 0.3): still create fallback
- Fallback link is unidirectional (only A → B, not B → A)

---

#### US-006: Threshold Strategy Remains Available

**As a** Fortemi user with existing deployment
**I want** the original threshold-based linking to remain available
**So that** I can choose based on my knowledge base characteristics

**Acceptance Criteria**:
- Given `GRAPH_LINKING_STRATEGY=threshold` in `.env`
- When linking job runs for note A
- Then system finds all notes with similarity >= threshold (e.g., 0.7)
- And creates bidirectional links to ALL matching notes
- And behavior is identical to pre-issue-386 implementation

**Edge Cases**:
- Empty string or missing env var defaults to threshold strategy
- Unknown strategy name logs warning and uses threshold

---

### Metrics and Observability Stories

#### US-007: Expose Graph Topology Metrics via API

**As a** system administrator or monitoring tool
**I want** quantitative graph topology metrics via REST API
**So that** I can measure impact of linking strategy changes

**Acceptance Criteria**:
- Given knowledge base contains >= 10 notes with links
- When I send `GET /api/v1/graph/topology/stats`
- Then response includes:
  ```json
  {
    "total_notes": 1247,
    "total_links": 8729,
    "avg_degree": 7.0,
    "degree_std_dev": 2.1,
    "clustering_coefficient": 0.42,
    "avg_path_length": 3.2,
    "topology_type": "mesh"
  }
  ```
- And response time is < 2 seconds for graphs up to 10,000 notes

**Edge Cases**:
- Graph with zero links returns `topology_type: "disconnected"`
- Very small graph (< 10 notes) includes warning flag
- Large graph (> 10k notes) uses sampling with `sampled: true` flag

---

#### US-008: Classify Topology Type Automatically

**As a** graph analyst
**I want** the system to classify topology as "star", "mesh", or "mixed"
**So that** I can quickly assess graph structure health

**Acceptance Criteria**:
- Given graph metrics are computed
- When `avg_degree_std_dev > 5` and `clustering < 0.1`
- Then `topology_type` is classified as "star"
- When `clustering >= 0.3` and `avg_degree_std_dev < 3`
- Then `topology_type` is classified as "mesh"
- Otherwise `topology_type` is "mixed"

**Edge Cases**:
- Metrics exactly at boundary (e.g., clustering = 0.3): classified as "mesh"
- Disconnected graph classified as "disconnected" regardless of other metrics

---

#### US-009: Log Linking Job Outcomes with Strategy Context

**As a** system operator
**I want** linking job logs to include strategy and k value
**So that** I can debug linking behavior and track strategy changes over time

**Acceptance Criteria**:
- Given linking job completes successfully
- When I review structured logs
- Then each job log includes:
  ```json
  {
    "level": "info",
    "subsystem": "jobs",
    "component": "linking",
    "note_id": "550e8400-e29b-41d4-a716-446655440000",
    "strategy": "mutual_knn",
    "k_value": 7,
    "links_created": 14,
    "wiki_links_found": 2,
    "duration_ms": 127
  }
  ```

**Edge Cases**:
- Failed jobs include error reason in structured field
- Fallback events logged separately with `fallback: true` flag

---

### Migration and Compatibility Stories

#### US-010: Preserve Existing Links During Strategy Change

**As a** Fortemi administrator
**I want** existing semantic links to remain intact when I change linking strategy
**So that** users don't lose established graph connections

**Acceptance Criteria**:
- Given knowledge base has 500 notes with threshold-based links
- When I change `GRAPH_LINKING_STRATEGY` from `threshold` to `mutual_knn`
- And restart Fortemi
- Then all 500 existing notes retain their current links
- And only NEW notes created after restart use mutual k-NN
- And GET `/api/v1/notes/{id}/links` returns unchanged link lists for existing notes

**Edge Cases**:
- Wiki-style links ([[title]]) are never affected by strategy changes
- Manual links (created via API) are preserved
- Only semantic links created by new linking jobs use new strategy

---

#### US-011: Bulk Re-linking Job (Optional Feature)

**As a** system administrator
**I want** an API endpoint to re-link all notes with a new strategy
**So that** I can migrate existing knowledge bases to mutual k-NN topology

**Acceptance Criteria**:
- Given knowledge base has existing threshold-based links
- When I send:
  ```
  POST /api/v1/graph/relink
  {
    "strategy": "mutual_knn",
    "k": 7,
    "apply_to_existing": true,
    "clear_semantic_links": true
  }
  ```
- Then system:
  1. Deletes all semantic links (preserves wiki and manual links)
  2. Triggers linking job for each note using mutual k-NN
  3. Returns job ID for progress tracking
- And job summary includes: `{notes_processed: 500, links_removed: 6234, links_created: 3500}`

**Edge Cases**:
- `apply_to_existing: false` skips re-linking (only affects new notes)
- `clear_semantic_links: false` appends new links to existing (not recommended)
- Job can be cancelled mid-execution without corrupting graph

---

### Error Handling and Resilience Stories

#### US-012: Graceful Degradation on k-NN Query Failure

**As a** Fortemi instance
**I want** linking jobs to succeed even if k-NN computation fails
**So that** transient database issues don't prevent note creation

**Acceptance Criteria**:
- Given linking job is executing for note A
- When k-NN query times out or returns database error
- Then system logs warning: `k-NN query failed, falling back to threshold strategy`
- And system attempts threshold-based linking as fallback
- And job status is `success_with_warnings`
- And job metadata includes: `{"fallback_used": "threshold", "reason": "knn_query_timeout"}`

**Edge Cases**:
- Both k-NN and threshold fail: job marked as failed with retry recommendation
- Embedding missing for note: skip semantic linking entirely (not a failure)

---

#### US-013: Detect and Report Isolated Nodes

**As a** knowledge graph curator
**I want** to be notified when notes fail to link to any others
**So that** I can review and manually connect isolated content

**Acceptance Criteria**:
- Given linking job completes for note A
- When both mutual k-NN and fallback linking create zero links
- Then system emits warning event:
  ```json
  {
    "event": "isolated_node_detected",
    "note_id": "...",
    "reason": "no_candidates_found"
  }
  ```
- And note is tagged with `system:isolated` for batch review
- And GET `/api/v1/graph/health` includes isolated node count in response

**Edge Cases**:
- First note in corpus is not tagged as isolated (expected state)
- Note becomes isolated after related notes are deleted: re-run linking job to resolve

---

## Traceability Matrix

| Use Case | User Stories | API Endpoints | Database Changes |
|----------|--------------|---------------|------------------|
| UC-001 | US-004, US-005 | `explore_graph` (MCP) | None (query-only) |
| UC-002 | US-001, US-003, US-006 | None (config-only) | None |
| UC-003 | US-002 | None | None |
| UC-004 | US-005, US-013 | None | Link metadata field |
| UC-005 | US-007, US-008 | `GET /api/v1/graph/topology/stats` | None (query-only) |
| UC-006 | US-010, US-011 | `POST /api/v1/graph/relink` | Bulk link update |
| UC-007 | US-009 | `GET /api/v1/analytics/graph/traversal-depth` | Event logging table |
| UC-008 | US-012 | None (internal) | None |

---

## Non-Functional Requirements

### Performance

- **NFR-PERF-001**: Linking job completes in < 500ms for notes in corpora up to 10,000 notes (95th percentile)
- **NFR-PERF-002**: Topology stats endpoint responds in < 2s for graphs up to 10,000 notes
- **NFR-PERF-003**: Topology stats endpoint responds in < 10s for graphs up to 100,000 notes (with sampling)
- **NFR-PERF-004**: k-NN query uses HNSW index (no full table scan)

### Reliability

- **NFR-REL-001**: Linking job failures do not corrupt existing links
- **NFR-REL-002**: Fallback logic ensures >= 99% of notes have at least one link (excludes first note in corpus)
- **NFR-REL-003**: Configuration changes require restart (no hot-reload) to prevent inconsistent state

### Compatibility

- **NFR-COMPAT-001**: Threshold-based linking remains available as default for backward compatibility
- **NFR-COMPAT-002**: Existing links are never modified by configuration changes (append-only)
- **NFR-COMPAT-003**: Migration path provided for users wanting to adopt mutual k-NN

### Observability

- **NFR-OBS-001**: All linking jobs emit structured logs with strategy, k, and outcome
- **NFR-OBS-002**: Topology metrics exposed via Prometheus-compatible endpoint
- **NFR-OBS-003**: Graph health warnings logged when isolated nodes exceed 5% of corpus

---

## Open Questions

1. **Re-linking API Scope**: Should `POST /api/v1/graph/relink` be included in MVP or deferred to post-launch?
   - **Impact**: MVP can ship without migration path for existing deployments
   - **Recommendation**: Defer to v2 unless user demand is high

2. **k-NN Index Optimization**: Should we pre-compute k-NN graph and cache results?
   - **Impact**: Reduces per-note linking latency but adds complexity
   - **Recommendation**: Measure before optimizing (HNSW index may be sufficient)

3. **Topology Classification Thresholds**: Are `clustering >= 0.3` and `std_dev < 3` appropriate for all corpus sizes?
   - **Impact**: Misclassification affects monitoring and alerting
   - **Recommendation**: Validate thresholds with test data across 100-10k note range

4. **Link Metadata Schema**: Should fallback links be distinguished in UI or only in metadata?
   - **Impact**: Users may want to hide/filter fallback links
   - **Recommendation**: Store in metadata first, add UI filter if user feedback requests it

---

## Success Metrics

### Quantitative (Graph Theory)

| Metric | Baseline (Threshold) | Target (Mutual k-NN) | Measurement Method |
|--------|---------------------|----------------------|-------------------|
| Clustering Coefficient | ≈ 0.0-0.1 | 0.3-0.6 | `GET /api/v1/graph/topology/stats` |
| Average Degree | 12-20 (bimodal) | 5-10 (uniform) | Degree distribution query |
| Degree Std Dev | > 5 (high variance) | < 3 (low variance) | Standard deviation of degrees |
| Avg Path Length | ≈ 2.0 | 3-4 | Sampled shortest paths |
| Isolated Nodes | < 5% | < 2% | Notes with zero links |

### Qualitative (User Experience)

| Metric | Baseline | Target | Measurement Method |
|--------|----------|--------|-------------------|
| Multi-hop Navigation | < 10% depth > 1 | > 30% depth > 1 | Link click analytics |
| Unexpected Discovery | Low (anecdotal) | Moderate (user feedback) | Survey after 30 days |
| Configuration Clarity | N/A (no option) | > 90% admin comprehension | Documentation review |

---

## Appendix: Configuration Reference

### Environment Variables

```bash
# Linking strategy selection
GRAPH_LINKING_STRATEGY=mutual_knn  # Options: threshold, mutual_knn, auto
                                   # Default: threshold

# k value for mutual k-NN (optional, auto-computed if unset)
GRAPH_K_NEIGHBORS=7                # Range: 3-50, Default: adaptive (log₂(N))

# Minimum similarity for fallback links (optional)
GRAPH_MIN_FALLBACK_SCORE=0.3       # Range: 0.0-1.0, Default: 0.3

# Enable topology metrics endpoint (optional)
GRAPH_TOPOLOGY_METRICS=true        # Default: true
```

### Strategy Comparison

| Strategy | Pros | Cons | Use Case |
|----------|------|------|----------|
| **threshold** | Simple, predictable | Creates star topology | Small corpora (< 100 notes) |
| **mutual_knn** | Mesh topology, bounded degree | May isolate outliers | General purpose (100+ notes) |
| **auto** (future) | Adapts strategy to corpus | Complex decision logic | Mixed content types |

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 0.1 | 2026-02-14 | Requirements Analyst Agent | Initial draft with 8 use cases, 13 user stories |

---

**Related Documents**:
- Research: `docs/research/graph-topology-executive-summary.md`
- Implementation Guide: `docs/research/graph-topology-implementation-guide.md`
- Technical Research: `docs/research/knowledge-graph-topology-techniques.md`

**Next Steps**:
1. Review use cases and user stories with stakeholders
2. Validate NFRs against production constraints
3. Prioritize stories for MVP (recommend: US-001, US-002, US-004, US-005, US-007)
4. Create API specification for new endpoints (topology stats, optional re-linking)
