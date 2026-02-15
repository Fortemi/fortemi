# Risk Assessment and Migration Plan

**Issue**: #386 -- Graph Topology Improvement (Threshold to Mutual k-NN)
**Date**: 2026-02-14
**Status**: Pre-Implementation
**Scope**: Replace threshold-based auto-linking (cosine >= 0.7) with mutual k-NN strategy

---

## 1. Risk Register

### RISK-001: Performance Regression from k+1 Queries per Note

| Field | Value |
|-------|-------|
| **Category** | Performance |
| **Description** | The current `LinkingHandler` issues one `find_similar` call per note with a fixed limit of 10 candidates. Mutual k-NN requires: (1) forward k-NN query for the new note, then (2) a reverse k-NN check for each candidate to verify mutuality. This changes the query pattern from 1 query to k+1 queries per linking job. With adaptive k = log2(N) clamped to [5, 15], a corpus of 32,768 notes produces k=15, meaning 16 queries per job. |
| **Probability** | Medium |
| **Impact** | High |
| **Risk Score** | Medium-High |
| **Mitigation** | Batch the reverse k-NN checks into a single SQL query using `ANY($1::uuid[])` rather than N+1 individual lookups. The reverse check can be expressed as: "for each candidate C, is the new note N among C's k nearest neighbors?" This is a single vector similarity query with a WHERE clause filtering to the candidate set. Profile the query plan with `EXPLAIN ANALYZE` on corpora of 1K, 10K, and 50K notes before merging. |
| **Contingency** | If linking latency exceeds 2s (backtracking trigger from ADR), revert `GRAPH_STRATEGY` to `threshold` via env var. No code deployment required. |
| **Owner** | Engineering |
| **Status** | Open |

### RISK-002: Isolated Node Rate Higher Than Expected

| Field | Value |
|-------|-------|
| **Category** | User Experience |
| **Description** | Mutual k-NN is strictly more selective than threshold linking. A note N only links to candidate C if both N is in C's k-NN and C is in N's k-NN. Notes with unusual content (niche topics, short notes, mixed-language content) may fail the mutuality check for all candidates, producing isolated nodes with zero semantic links. The ADR backtracking trigger is >15% isolated nodes. |
| **Probability** | Medium |
| **Impact** | Medium |
| **Risk Score** | Medium |
| **Mitigation** | Implement the isolated node fallback described in the ADR: if a note has zero mutual neighbors, fall back to asymmetric k-NN (top-1 nearest neighbor without mutuality requirement) to guarantee at least one link. Log fallback invocations as a distinct metric so the rate can be monitored. |
| **Contingency** | If isolated rate exceeds 15% despite fallback, lower the mutuality strictness by accepting "near-mutual" links (candidate is in top 2k instead of top k). Alternatively revert to threshold. |
| **Owner** | Engineering |
| **Status** | Open |

### RISK-003: Mixed Topology During Gradual Migration

| Field | Value |
|-------|-------|
| **Category** | Data |
| **Description** | During Phase 2, new notes use mutual k-NN while existing notes retain threshold-generated links. The graph will contain two link populations with different density characteristics: threshold links are dense (any pair above 0.7 gets linked), k-NN links are sparse (bounded by k). This creates an asymmetric graph where older notes appear disproportionately connected. Graph traversal queries (recursive CTE in `crates/matric-db/src/links.rs`) and the health dashboard's "unlinked notes" metric will reflect this mixed state. |
| **Probability** | High |
| **Impact** | Low |
| **Risk Score** | Medium-Low |
| **Mitigation** | Store the linking strategy in `link.metadata` JSONB as `{"strategy": "threshold"}` or `{"strategy": "mutual_knn", "k": 8}`. This enables filtering, reporting, and selective re-linking. The existing schema supports this without migration since `metadata JSONB DEFAULT '{}'::jsonb` is already present on the `link` table. |
| **Contingency** | If the mixed topology causes user confusion, run the Phase 3 bulk re-link earlier than planned to normalize the graph. The re-link job can run in the background without downtime. |
| **Owner** | Engineering |
| **Status** | Open |

### RISK-004: k Value Sensitivity Across Corpus Types

| Field | Value |
|-------|-------|
| **Category** | Technical |
| **Description** | Adaptive k = log2(N) clamped to [5, 15] assumes a roughly uniform distribution of content. Specialized corpora (e.g., a memory archive dedicated entirely to Rust code) may cluster tightly in embedding space, making k=5 too low (star topology re-emerges) or k=15 too high (noise links). The current `defaults.rs` already distinguishes code vs. prose thresholds (0.85 vs 0.70), suggesting content-type sensitivity is a real concern. |
| **Probability** | Medium |
| **Impact** | Medium |
| **Risk Score** | Medium |
| **Mitigation** | Make k overridable per memory archive via archive-level configuration (stored in archive metadata). The `GraphConfig` struct should accept `GRAPH_K_MIN`, `GRAPH_K_MAX` env vars with the [5, 15] defaults. For code-heavy archives, operators can tighten to [3, 8]. Document the tuning guidance in `docs/content/`. |
| **Contingency** | If a specific archive produces poor topology, override k for that archive. The multi-memory architecture already supports per-archive configuration patterns. |
| **Owner** | Engineering |
| **Status** | Open |

### RISK-005: Reverse k-NN Check Creates N+1 Query Pattern

| Field | Value |
|-------|-------|
| **Category** | Performance |
| **Description** | The naive implementation of mutual k-NN checks mutuality by running `find_similar` from each candidate's perspective. With k=15 candidates, this means 15 additional vector similarity searches against the full embedding table. Each `find_similar` currently uses pgvector's `<=>` operator with an IVFFlat or HNSW index scan. The cumulative cost could dominate the linking job's execution time, especially on corpora exceeding 10K notes. |
| **Probability** | High |
| **Impact** | High |
| **Risk Score** | High |
| **Mitigation** | Replace per-candidate reverse queries with a single batched check. For each candidate C with embedding vector v_C, compute `find_similar(v_C, k)` and check if note N appears in the result set. This can be batched: for all candidates, run a single query that returns the k-NN sets for all candidate vectors in one round trip using `LATERAL` joins or a CTE with `unnest`. Alternatively, maintain a pre-computed reverse k-NN index (materialized view refreshed on embedding insert). |
| **Contingency** | If batching is insufficient, implement an approximate mutuality check: accept the link if the forward similarity score exceeds a secondary threshold (e.g., 0.6), bypassing the reverse check. This is a controlled relaxation that still improves over pure threshold linking. |
| **Owner** | Engineering |
| **Status** | Open |

### RISK-006: Topology Stats Computation Timeout on Large Graphs

| Field | Value |
|-------|-------|
| **Category** | Performance |
| **Description** | The new topology metrics endpoint must compute clustering coefficient, degree distribution, and connected components. Clustering coefficient requires examining the neighborhood of every node (O(N * k^2) in the worst case). Connected components via recursive CTE can be expensive on dense graphs. For a corpus of 50K notes with k=15, this is non-trivial. The existing `JOB_TIMEOUT_SECS` is 300s (5 minutes). |
| **Probability** | Medium |
| **Impact** | Medium |
| **Risk Score** | Medium |
| **Mitigation** | Compute topology metrics as a background job (not synchronous API call). Cache results with a TTL (e.g., 15 minutes). Use sampling for large graphs: compute clustering coefficient on a random 10% sample and extrapolate. For connected components, use a SQL-native union-find approach rather than recursive CTE. |
| **Contingency** | If computation exceeds 30s even with sampling, degrade to returning only degree distribution (O(N) via `GROUP BY from_note_id`) and skip clustering coefficient for graphs over 20K nodes. |
| **Owner** | Engineering |
| **Status** | Open |

### RISK-007: User Perception of Fewer Links (Quality vs. Quantity)

| Field | Value |
|-------|-------|
| **Category** | User Experience |
| **Description** | Threshold linking at 0.7 creates many links (any pair above threshold). Mutual k-NN creates fewer, higher-quality links. Users and agents accustomed to seeing 5-10 links per note may perceive the change as a regression. The MCP `search` tool and graph exploration features surface link counts prominently. |
| **Probability** | Medium |
| **Impact** | Low |
| **Risk Score** | Low-Medium |
| **Mitigation** | Document the change in release notes with concrete examples showing improved link quality. Add a `link_strategy` field to link list responses so users and agents can distinguish threshold from k-NN links. The MCP `get_system_info` tool should report the active linking strategy. |
| **Contingency** | If user feedback is negative, increase k bounds to [8, 20] to produce more links while maintaining the mutual quality filter. |
| **Owner** | Product |
| **Status** | Open |

### RISK-008: Config Drift Between Memory Archives

| Field | Value |
|-------|-------|
| **Category** | Operational |
| **Description** | With multi-memory architecture (schema-level isolation), each archive could theoretically run a different linking strategy. If `GRAPH_STRATEGY` is a global env var, all archives use the same strategy. But if Phase 2 enables k-NN for "new notes only," the definition of "new" varies per archive depending on when it was created and last re-linked. Operators managing multiple archives may lose track of which archives have been re-linked. |
| **Probability** | Low |
| **Impact** | Medium |
| **Risk Score** | Low-Medium |
| **Mitigation** | Store the active linking strategy and last re-link timestamp in archive metadata. Expose this in the `GET /api/v1/archives` response. The topology metrics endpoint (per-archive) should include `strategy_distribution` showing the percentage of links created by each strategy. |
| **Contingency** | Provide a `POST /api/v1/archives/:id/relink` endpoint to trigger bulk re-linking for a specific archive, allowing operators to normalize individual archives on demand. |
| **Owner** | Operations |
| **Status** | Open |

### RISK-009: Backward Compatibility Break in Edge Cases

| Field | Value |
|-------|-------|
| **Category** | Technical |
| **Description** | The current `LinkingHandler` creates bidirectional links unconditionally for all pairs above threshold (lines 821-843 of `jobs.rs`). External integrations or agent workflows may depend on this behavior, specifically: (a) every note getting at least one semantic link if any similar content exists above 0.7, and (b) links being symmetric (if A->B exists, B->A exists). Mutual k-NN preserves symmetry but may produce fewer links, breaking assumption (a). |
| **Probability** | Low |
| **Impact** | Medium |
| **Risk Score** | Low-Medium |
| **Mitigation** | Phase 1 deploys with `strategy=threshold` as default, preserving backward compatibility. The strategy switch is opt-in via `GRAPH_STRATEGY=mutual_knn`. Document the behavioral difference in the API changelog. The `link.metadata` JSONB field will tag all new links with their creation strategy, enabling API consumers to filter if needed. |
| **Contingency** | If a specific integration breaks, that integration can filter links by `metadata->>'strategy'` to include only threshold-generated links while the integration is updated. |
| **Owner** | Engineering |
| **Status** | Open |

### RISK-010: Embedding Model Change Invalidates k-NN Relationships

| Field | Value |
|-------|-------|
| **Category** | Data |
| **Description** | k-NN relationships are defined by relative distances in embedding space. If the embedding model changes (e.g., from `nomic-embed-text` at 768 dimensions to a different model), all existing k-NN links become meaningless because the distance relationships change. The existing threshold approach has the same theoretical problem, but k-NN is more sensitive because it relies on relative ordering rather than absolute scores. The system already supports multiple embedding sets (`crates/matric-db/src/embedding_sets.rs`), making model changes a supported operation. |
| **Probability** | Low |
| **Impact** | High |
| **Risk Score** | Medium |
| **Mitigation** | Tie link validity to the embedding set that produced them. Store `embedding_set_id` in `link.metadata` when creating k-NN links. When an embedding set is replaced or a model change occurs, mark associated links as stale. The `ReEmbedAllHandler` (already registered in `main.rs` at line 969) should trigger a bulk re-link after re-embedding completes. |
| **Contingency** | If stale links cause issues before re-linking completes, the topology metrics endpoint should report the percentage of links tied to the current vs. previous embedding set, giving operators visibility into the transition state. |
| **Owner** | Engineering |
| **Status** | Open |

---

## 2. Migration Plan

### Phase 1: Deploy with strategy=threshold (Zero Risk)

**Duration**: Immediate (ships with the PR)
**Objective**: Deploy all new code with the default strategy set to `threshold`, producing identical behavior to the current system.

**Steps**:

1. Add `GraphConfig` struct to `crates/matric-core/src/defaults.rs`:
   - `GRAPH_STRATEGY` env var, default `"threshold"`
   - `GRAPH_K_MIN` env var, default `5`
   - `GRAPH_K_MAX` env var, default `15`
   - `GRAPH_ISOLATED_FALLBACK` env var, default `true`

2. Modify `LinkingHandler` in `crates/matric-api/src/handlers/jobs.rs`:
   - Add strategy dispatch at the top of `execute()`
   - `"threshold"` branch runs the existing code path (lines 800-844 unchanged)
   - `"mutual_knn"` branch runs the new algorithm
   - Both branches write `strategy` to `link.metadata`

3. Add topology stats handler:
   - New endpoint `GET /api/v1/stats/topology`
   - Returns: node count, edge count, degree distribution, isolated node count, clustering coefficient (sampled)
   - Scoped to active memory archive via `X-Fortemi-Memory` header

4. Add topology SQL queries to `crates/matric-db/src/links.rs`:
   - `degree_distribution()` -- `GROUP BY from_note_id`
   - `isolated_nodes()` -- notes with zero semantic links
   - `clustering_coefficient_sample(sample_size)` -- sampled neighborhood analysis

**Verification**:
- All existing tests pass without modification
- `cargo test --workspace` green
- Linking behavior identical (threshold is default)
- New topology endpoint returns baseline metrics

**Rollback**: Not needed -- no behavior change.

### Phase 2: Enable mutual_knn for New Notes

**Duration**: 1-2 weeks after Phase 1 deployment
**Objective**: Validate mutual k-NN on live traffic for newly created notes.

**Steps**:

1. Set `GRAPH_STRATEGY=mutual_knn` in `.env`
2. Restart the API (`docker compose -f docker-compose.bundle.yml up -d`)
3. Monitor for 2 weeks:
   - Linking job latency (via job worker logs, `duration_ms` field)
   - Isolated node rate (via topology endpoint)
   - Clustering coefficient trend
   - User feedback on link quality

**Acceptance Criteria** (from ADR backtracking triggers):
- Isolated nodes < 15%
- Clustering coefficient > 0.2
- Linking job latency < 2s per note

**Monitoring Queries**:
```sql
-- Isolated node rate
SELECT
  COUNT(*) FILTER (WHERE link_count = 0) AS isolated,
  COUNT(*) AS total,
  ROUND(100.0 * COUNT(*) FILTER (WHERE link_count = 0) / COUNT(*), 2) AS isolated_pct
FROM (
  SELECT n.id, COUNT(l.id) AS link_count
  FROM note n
  LEFT JOIN link l ON l.from_note_id = n.id AND l.kind = 'semantic'
  WHERE n.archived_at_utc IS NULL
  GROUP BY n.id
) sub;

-- Strategy distribution
SELECT
  COALESCE(metadata->>'strategy', 'threshold') AS strategy,
  COUNT(*) AS link_count
FROM link
WHERE kind = 'semantic'
GROUP BY 1;

-- Linking job latency (last 100 jobs)
SELECT
  AVG(EXTRACT(EPOCH FROM (completed_at_utc - started_at_utc))) AS avg_secs,
  MAX(EXTRACT(EPOCH FROM (completed_at_utc - started_at_utc))) AS max_secs,
  PERCENTILE_CONT(0.95) WITHIN GROUP (
    ORDER BY EXTRACT(EPOCH FROM (completed_at_utc - started_at_utc))
  ) AS p95_secs
FROM job
WHERE job_type = 'linking' AND status = 'completed'
ORDER BY completed_at_utc DESC
LIMIT 100;
```

**Rollback**: Set `GRAPH_STRATEGY=threshold` in `.env` and restart. New notes will resume threshold linking. Existing k-NN links remain valid and are not removed.

### Phase 3: Optional Bulk Re-link for Existing Corpus

**Duration**: After Phase 2 acceptance criteria are met
**Objective**: Normalize the graph by re-linking all existing notes with mutual k-NN.

**Steps**:

1. Add a `POST /api/v1/jobs/relink-all` endpoint (or extend `ReEmbedAllHandler`)
2. The re-link job iterates all notes and runs the mutual k-NN linking for each
3. Old threshold links are soft-deprecated (not deleted) by adding `{"superseded": true}` to their metadata
4. New k-NN links are created alongside
5. After validation, a cleanup job removes superseded links

**Considerations**:
- For a corpus of 10K notes, this produces 10K linking jobs. At 2s per job, total time is approximately 5.5 hours. Run during off-peak hours.
- Use the job queue's priority system (`AUTO_EMBED_PRIORITY`) to run re-link jobs at low priority so they do not block new note processing.
- The re-link is idempotent -- running it twice produces the same result because `link` table has a uniqueness check on `(from_note_id, to_note_id, kind)`.

**Rollback**: Restore superseded threshold links by removing the `{"superseded": true}` flag. Delete links with `{"strategy": "mutual_knn"}` metadata.

---

## 3. Rollback Strategy

### Immediate Rollback (Environment Variable)

**Procedure**:
1. Set `GRAPH_STRATEGY=threshold` in `.env`
2. Restart: `docker compose -f docker-compose.bundle.yml down && docker compose -f docker-compose.bundle.yml up -d`
3. Verify: new linking jobs produce threshold-style links

**Time to rollback**: Under 2 minutes.

### Impact of Rollback on Mixed-Strategy Graphs

After rollback, the graph contains both threshold and k-NN links. This is safe because:

- **Link table is additive**: both strategies produce valid `(from_note_id, to_note_id, kind="semantic", score)` rows. The schema is strategy-agnostic.
- **No schema migration**: the `link` table structure is unchanged. The `metadata` JSONB column stores strategy provenance but is not required by any query.
- **Graph traversal is strategy-agnostic**: the recursive CTE in link traversal queries joins on `from_note_id`/`to_note_id` regardless of how the link was created.
- **Search is unaffected**: semantic search uses the `embedding` table, not the `link` table.

### Data Preservation Guarantees

| Guarantee | Mechanism |
|-----------|-----------|
| No links are deleted during strategy switch | Strategy dispatch only creates new links |
| Old threshold links remain valid | No migration modifies existing rows |
| Link provenance is recorded | `metadata` JSONB tags each link with its creation strategy |
| Rollback does not require re-linking | Threshold mode resumes creating threshold links for new notes |
| Bulk re-link (Phase 3) is reversible | Superseded links are soft-deprecated, not deleted |

### Full Rollback (Code Revert)

If the mutual k-NN code itself has bugs:

1. Revert the PR via `git revert`
2. Deploy the reverted code
3. Existing k-NN links remain in the database but are inert (new links will be threshold-only)
4. Optionally clean up k-NN links: `DELETE FROM link WHERE metadata->>'strategy' = 'mutual_knn';`

---

## 4. Risk Mitigation Timeline

### Pre-Implementation (Before PR Merge)

| Action | Target Date | Owner | Status |
|--------|-------------|-------|--------|
| Collect baseline topology metrics on production corpus | Before PR | Engineering | Pending |
| Run `isolated_nodes` query to establish current isolation rate | Before PR | Engineering | Pending |
| Profile `find_similar` query plan at current corpus size | Before PR | Engineering | Pending |
| Document current average linking job duration | Before PR | Engineering | Pending |
| Review pgvector HNSW index configuration for k-NN workload | Before PR | Engineering | Pending |

### During Implementation (PR Development)

| Action | Target Date | Owner | Status |
|--------|-------------|-------|--------|
| Implement `GraphConfig` with env var parsing and defaults | Sprint 1 | Engineering | Pending |
| Implement mutual k-NN branch in `LinkingHandler` | Sprint 1 | Engineering | Pending |
| Implement batched reverse k-NN check (RISK-005 mitigation) | Sprint 1 | Engineering | Pending |
| Implement isolated node fallback (RISK-002 mitigation) | Sprint 1 | Engineering | Pending |
| Add strategy tag to `link.metadata` (RISK-003 mitigation) | Sprint 1 | Engineering | Pending |
| Implement topology stats endpoint | Sprint 1 | Engineering | Pending |
| Write unit tests for k-NN logic with mock embeddings | Sprint 1 | Engineering | Pending |
| Write integration tests comparing threshold vs. k-NN output | Sprint 1 | Engineering | Pending |
| Load test with 10K and 50K note corpora | Sprint 1 | Engineering | Pending |

### Post-Deployment: Monitoring Period (2 Weeks)

| Action | Schedule | Owner | Status |
|--------|----------|-------|--------|
| Check isolated node rate daily via topology endpoint | Daily | Operations | Pending |
| Review linking job latency (p50, p95, p99) | Daily | Engineering | Pending |
| Compare clustering coefficient to baseline | Weekly | Engineering | Pending |
| Collect user/agent feedback on link quality | Ongoing | Product | Pending |
| Review strategy distribution (threshold vs. k-NN link counts) | Weekly | Engineering | Pending |

### Acceptance Criteria for Full Rollout (Phase 3)

All of the following must be met for at least 7 consecutive days:

| Criterion | Threshold | Source |
|-----------|-----------|--------|
| Isolated node rate | < 15% | Topology endpoint |
| Clustering coefficient | > 0.2 | Topology endpoint |
| Linking job p95 latency | < 2s | Job worker logs |
| No regression in graph traversal query time | Within 20% of baseline | Application metrics |
| No user-reported link quality issues | Zero critical reports | Support channel |

If any criterion is breached, pause rollout and investigate. If two or more criteria are breached simultaneously, rollback to threshold and reassess the approach.

---

## Appendix A: Current Implementation Reference

The following code locations are directly affected by this change:

| File | Lines | Current Behavior |
|------|-------|------------------|
| `crates/matric-api/src/handlers/jobs.rs` | 666-858 | `LinkingHandler` -- threshold-based with `find_similar(vec, 10, true)` and bidirectional link creation for all hits above `link_threshold` |
| `crates/matric-core/src/defaults.rs` | 284-320 | `SEMANTIC_LINK_THRESHOLD` (0.7), `SEMANTIC_LINK_THRESHOLD_CODE` (0.85), `semantic_link_threshold_for()` |
| `crates/matric-db/src/links.rs` | 24-56 | `PgLinkRepository::create()` with dedup via `WHERE NOT EXISTS` |
| `crates/matric-db/src/embeddings.rs` | 68+ | `find_similar()` -- pgvector cosine distance search |
| `crates/matric-api/src/main.rs` | 969 | Handler registration: `.register_handler(LinkingHandler::new(db.clone()))` |

## Appendix B: Link Table Schema

```sql
CREATE TABLE link (
  id UUID PRIMARY KEY,
  from_note_id UUID REFERENCES note(id) ON DELETE CASCADE,
  to_note_id UUID,
  to_url TEXT,
  kind TEXT NOT NULL,          -- 'semantic', 'wiki', 'manual'
  score REAL NOT NULL,
  created_at_utc TIMESTAMPTZ NOT NULL,
  metadata JSONB DEFAULT '{}'::jsonb,  -- strategy provenance stored here
  FOREIGN KEY (to_note_id) REFERENCES note(id) ON DELETE CASCADE,
  CHECK ((to_note_id IS NOT NULL AND to_url IS NULL) OR
         (to_note_id IS NULL AND to_url IS NOT NULL))
);
```

The `metadata` JSONB column is the extension point for this change. No schema migration required.

## Appendix C: ADR Decision Summary

| Strategy | Score | Strengths | Weaknesses |
|----------|-------|-----------|------------|
| Threshold (current) | 3.25 | Simple, fast, predictable | Star topology, low clustering |
| Asymmetric k-NN | 3.75 | Better topology, moderate complexity | One-sided links possible |
| **Mutual k-NN (selected)** | **4.10** | Best topology, symmetric by design | Higher query cost, possible isolation |

Backtracking triggers: isolated nodes >15%, clustering <0.2, latency >2s.
