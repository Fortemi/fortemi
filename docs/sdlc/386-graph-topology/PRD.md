# Product Requirements Document: Graph Topology Improvement

**Document Status**: Draft
**Issue**: #386
**Created**: 2026-02-14
**Author**: Product Strategist
**Stakeholders**: Engineering Team, Product Owner

---

## Executive Summary

Fortemi's current auto-linking strategy produces star topologies where notes cluster densely around central hubs, limiting the utility of graph traversal for knowledge discovery. This PRD defines requirements for implementing mutual k-nearest neighbors (k-NN) linking to create mesh-of-stars topologies that enable meaningful multi-hop traversal while maintaining backward compatibility with existing knowledge bases.

**Success Criteria**: Clustering coefficient >0.3, average node degree 5-10, >30% of user navigation beyond depth=1.

---

## 1. Problem Statement

### 1.1 Current State

The auto-linking pipeline (`crates/matric-api/src/handlers/jobs.rs:665-863`) creates bidirectional semantic links between notes using cosine similarity thresholds:
- Prose content: 0.7 threshold
- Code content: 0.85 threshold (higher due to tighter embedding clusters)

### 1.2 Observed Issues

**Star Topology Formation**: In high-dimensional embedding spaces (768 dimensions for nomic-embed-text), notes on similar topics form tight clusters where all members exceed the similarity threshold when compared to a central exemplar. This creates:

- Dense intra-cluster connections (20+ links per note)
- Sparse inter-cluster bridges
- Shallow graph depth (average path length ≈ 2.0)
- Clustering coefficient ≈ 0.0 (no triangles)

**User Impact**: Graph traversal yields redundant results within the same topic rather than discovering unexpected cross-topic connections. Users rarely navigate beyond depth=1 (<10% of link clicks).

### 1.3 Root Cause

This is an architectural limitation of epsilon-threshold graphs, not a threshold tuning problem. Research literature (Dong 2011, Kleinberg 2000, Newman 2002) shows k-NN produces superior topology for knowledge graphs.

---

## 2. Product Vision

Enable knowledge discovery through serendipitous graph exploration by creating a mesh-of-stars topology with bounded node degree, small-world properties, and meaningful multi-hop traversal paths.

**Target Experience**: Users navigate from a technical note about PostgreSQL to a related business process document via semantic bridges, discovering connections that search alone would not surface.

---

## 3. User Personas

### 3.1 Primary: AI Agent

**Profile**: LLM-powered agent traversing the knowledge graph to gather context for question answering or content generation.

**Goals**:
- Discover semantically related notes within 2-3 hops
- Avoid redundant clustered content
- Find inter-topic bridges for cross-domain reasoning

**Pain Points**:
- Current star topology returns 20 notes on the same narrow topic
- Missing bridges between related domains (e.g., code implementation ↔ design rationale)

### 3.2 Secondary: Knowledge Base Administrator

**Profile**: User responsible for configuring and maintaining Fortemi deployment.

**Goals**:
- Tune linking strategy to corpus characteristics (size, domain diversity)
- Monitor graph health metrics (connectivity, clustering)
- Ensure new notes integrate properly into existing topology

**Pain Points**:
- No visibility into graph topology metrics
- No control over linking strategy
- Cannot diagnose why some notes remain isolated

### 3.3 Tertiary: Developer/Integrator

**Profile**: Engineer extending Fortemi's linking capabilities or integrating custom strategies.

**Goals**:
- Implement domain-specific linking heuristics
- Add new linking strategies via plugin architecture
- Benchmark alternative approaches

**Pain Points**:
- Linking logic tightly coupled to single implementation
- No abstraction for strategy pattern
- Limited extension points

---

## 4. Functional Requirements

### FR-1: Configurable Linking Strategy

**Priority**: P0 (Required)

**Description**: Support multiple linking strategies with runtime selection via environment configuration.

**Acceptance Criteria**:
- Environment variable `GRAPH_LINKING_STRATEGY` accepts values: `threshold` (default/legacy), `mutual_knn`, extensible for future strategies
- Strategy selection occurs at job execution time (no restart required for existing deployments)
- Invalid strategy value logs warning and falls back to `threshold`

**Technical Notes**:
- Implement strategy enum in `crates/matric-core/src/config.rs`
- Refactor `LinkingHandler::execute` to dispatch based on strategy
- See implementation guide sections 1-2

### FR-2: Mutual k-NN Linking Implementation

**Priority**: P0 (Required)

**Description**: Implement mutual k-nearest neighbors linking as recommended strategy.

**Acceptance Criteria**:
- Each note links to k most similar neighbors (configurable via `GRAPH_K_NEIGHBORS`, default 7)
- Links created only if relationship is mutual (both notes in each other's k-NN)
- Metadata includes `strategy: "mutual_knn"`, `k` value, forward/reverse rank positions
- Link type remains `semantic` for consistency with existing schema

**Technical Notes**:
- Leverages existing `embeddings.find_similar()` function
- Requires k+1 similarity searches per note (1 forward + k reverse checks)
- Expected latency: 50-100ms per note
- See implementation guide lines 18-103

### FR-3: Adaptive k Based on Corpus Size

**Priority**: P1 (High)

**Description**: Automatically adjust k based on total note count to maintain optimal topology across corpus sizes.

**Acceptance Criteria**:
- When `GRAPH_ADAPTIVE_K=true` (default), compute k as `log₂(N)` clamped to [5, 15]
- When `GRAPH_ADAPTIVE_K=false`, use fixed `GRAPH_K_NEIGHBORS` value
- Log computed k value at INFO level during job execution

**Technical Notes**:
- Research-backed heuristic ensures k scales with corpus size
- Prevents over-connection in small corpora (k=5 for <100 notes)
- Prevents under-connection in large corpora (k=15 for >10k notes)
- See implementation guide lines 105-135

### FR-4: Isolated Node Fallback

**Priority**: P1 (High)

**Description**: Prevent disconnected graph components by ensuring minimum connectivity for outlier notes.

**Acceptance Criteria**:
- If mutual k-NN produces zero links for a note AND similarity search returns candidates, create single link to best match
- Log fallback activation at WARN level with note ID
- Metadata includes `is_fallback: true` to distinguish from mutual links

**Technical Notes**:
- Addresses edge case where unique/outlier notes have no mutual k-NN
- Maintains weak connectivity while preserving mesh topology for majority of graph
- See implementation guide troubleshooting section

### FR-5: Graph Topology Metrics Endpoint

**Priority**: P1 (High)

**Description**: Expose topology metrics via API endpoint for monitoring and validation.

**Acceptance Criteria**:
- `GET /api/v1/graph/topology/stats` returns JSON with:
  - `total_nodes`: note count
  - `total_edges`: link count (bidirectional counted once)
  - `avg_degree`: mean links per node
  - `clustering_coefficient`: (triangles) / (connected triples)
  - `degree_distribution`: histogram of node degree counts
  - `topology_type`: classification (star, transitional, mesh, small_world)
- Response time <5 seconds for corpora up to 10k notes
- Endpoint accessible to authenticated users (respects `REQUIRE_AUTH`)

**Technical Notes**:
- Computation requires traversing all links (potentially expensive)
- Consider caching with TTL for large corpora
- See implementation guide lines 525-616

### FR-6: Backward Compatibility

**Priority**: P0 (Required)

**Description**: Preserve existing threshold-based linking as default behavior until explicit migration.

**Acceptance Criteria**:
- Default `GRAPH_LINKING_STRATEGY=threshold` maintains current behavior
- Existing links remain intact when switching strategies
- New strategies only affect links created after configuration change
- Dual-mode operation: different memories can use different strategies

**Technical Notes**:
- No database migration required (metadata column already supports arbitrary JSONB)
- Strategy information persists in `link.metadata` for audit trail
- Administrator can filter links by strategy via metadata queries

---

## 5. Non-Functional Requirements

### NFR-1: Performance Constraints

**Target**: Linking latency ≤200ms per note at 95th percentile

**Justification**: Background job must complete within reasonable timeframe to avoid queue buildup.

**Measurement**:
- Instrument `LinkingHandler::execute` with duration tracking
- Log at WARN level if execution exceeds 200ms
- Export latency histogram to metrics endpoint

**Technical Approach**:
- Mutual k-NN reuses HNSW index (same as current search)
- Parallel reverse k-NN checks via async tokio tasks
- Early termination if k mutual links found

### NFR-2: No New Dependencies

**Constraint**: Mutual k-NN implementation must use existing dependencies only

**Justification**: Minimize supply chain risk and maintenance burden

**Validation**:
- `cargo tree` before/after implementation shows no new crates
- pgvector + sqlx sufficient for k-NN queries
- No external graph libraries required for core functionality

**Note**: Future enhancements (community detection, RNG pruning) may require additional dependencies, but base k-NN must not.

### NFR-3: Configuration Without Restart

**Requirement**: Strategy changes apply to new linking jobs without API/worker restart

**Justification**: Enables A/B testing and gradual rollout without downtime

**Implementation**:
- Load configuration from environment on each job execution
- Do not cache strategy in long-lived structs
- Document that existing links are not retroactively modified

---

## 6. Success Metrics

### 6.1 Quantitative (Graph Theory)

Measured via `GET /api/v1/graph/topology/stats`:

| Metric | Baseline (Threshold) | Target (Mutual k-NN) | Measurement |
|--------|---------------------|---------------------|-------------|
| Clustering Coefficient | ≈0.0 | 0.3-0.6 | (triangles) / (connected triples) |
| Average Degree | Bimodal (0 or 20+) | 5-10 | (total links) / (total notes) |
| Degree Std Dev | High (star hubs) | Low (uniform) | Standard deviation of degree distribution |
| Topology Type | "star" | "mesh" or "small_world" | Qualitative classification |

### 6.2 Qualitative (User Engagement)

Measured via event tracking:

| Metric | Baseline | Target | Measurement |
|--------|----------|--------|-------------|
| Depth>1 Navigation | <10% of link clicks | >30% of link clicks | Count clicks on notes reached via ≥2 hops |
| Cross-Topic Exploration | <5% of sessions | >15% of sessions | Sessions visiting notes across 3+ document categories |
| Graph Traversal Duration | <30 seconds/session | >60 seconds/session | Time spent navigating via links vs. search |

### 6.3 System Health

| Metric | Target | Alert Threshold |
|--------|--------|----------------|
| Linking Job Latency | <100ms p50, <200ms p95 | >500ms p95 |
| Isolated Nodes | <5% of corpus | >10% of corpus |
| Links Per Note | 5-10 mean | <3 or >15 mean |

---

## 7. Out of Scope

The following enhancements are deferred to future phases:

### 7.1 RNG Edge Pruning

**Rationale**: Optimization adds complexity (O(N²) per note) without addressing stated problem. Evaluate post-k-NN if degree distribution still too high.

**Future Trigger**: Average degree >15 after k-NN deployment

### 7.2 Community Detection + Bridge Links

**Rationale**: Requires new dependency (petgraph), batch job infrastructure, and is primarily valuable for large corpora (>10k notes).

**Future Trigger**: Corpus size >5k notes and evidence of disconnected topic clusters

### 7.3 Hierarchical Linking

**Rationale**: Requires schema changes (link level tracking), different similarity thresholds per depth level, and multi-scale navigation UI.

**Future Trigger**: User research validates need for hierarchical knowledge exploration

### 7.4 Link Strength Decay

**Rationale**: Time-based or access-based link weight adjustment requires usage analytics infrastructure and introduces statefulness.

**Future Trigger**: Evidence that stale links reduce discovery quality

### 7.5 Retroactive Re-Linking

**Rationale**: Bulk re-processing of existing links is expensive and may disrupt user workflows. Strategy changes apply to new links only.

**Future Trigger**: User requests explicit migration tool for topology optimization

---

## 8. Dependencies

### 8.1 Internal

**None** - Feature builds on existing infrastructure:
- Embedding pipeline (`crates/matric-inference`)
- HNSW similarity search (`crates/matric-db/src/embeddings.rs`)
- Link schema (`migrations/20260102000000_initial_schema.sql:203-216`)

### 8.2 External

**None** - No new external dependencies for core mutual k-NN implementation.

### 8.3 Research Artifacts

Completed research in `/home/roctinam/dev/fortemi/docs/research/`:
- `knowledge-graph-topology-techniques.md` - Comprehensive analysis
- `graph-topology-implementation-guide.md` - Code examples
- `graph-topology-executive-summary.md` - Decision rationale

---

## 9. Risks and Mitigations

### 9.1 Risk: k Value Sensitivity

**Probability**: Medium
**Impact**: Medium

**Description**: Optimal k varies with corpus size and domain diversity. Fixed k may over-connect small corpora or under-connect large ones.

**Mitigation**:
- Implement adaptive k (FR-3) as default
- Expose manual override via `GRAPH_K_NEIGHBORS`
- Document k selection guidance in admin guide

**Residual Risk**: Domain-specific corpora (e.g., highly specialized vs. broad general knowledge) may need different k values. Future enhancement: per-memory k configuration.

### 9.2 Risk: Isolated Nodes

**Probability**: Low
**Impact**: Medium

**Description**: Outlier notes with unique topics may have zero mutual k-NN matches, creating disconnected graph components.

**Mitigation**:
- Implement fallback linking (FR-4)
- Monitor isolated node percentage via topology stats
- Alert if isolated nodes >10% of corpus

**Residual Risk**: Fallback creates asymmetric links (only one direction is mutual), which may confuse users expecting bidirectional semantics.

### 9.3 Risk: User Experience Disruption

**Probability**: Medium
**Impact**: Low

**Description**: Users accustomed to dense topic clusters may perceive sparser links as missing connections.

**Mitigation**:
- Maintain threshold strategy as default (backward compatible)
- Provide A/B testing capability via dual-mode operation
- Document topology trade-offs in user guide

**Acceptance**: Validated through A/B testing before default strategy change.

### 9.4 Risk: Performance Regression

**Probability**: Low
**Impact**: Medium

**Description**: k reverse similarity searches may exceed latency budget on large corpora.

**Mitigation**:
- Benchmark on realistic corpus sizes (100, 1k, 10k notes)
- Implement async parallel reverse checks
- Set latency alerting (NFR-1)

**Acceptance**: If p95 latency >200ms, defer k-NN or implement incremental optimization.

### 9.5 Risk: Incomplete Research Validation

**Probability**: Low
**Impact**: High

**Description**: Research recommendations may not translate to production topology improvements.

**Mitigation**:
- Prototype on test corpus before production deployment
- Measure baseline topology metrics pre-implementation
- Define rollback criteria (e.g., clustering coefficient <0.2 after 1 week)

**Validation**: Research is based on peer-reviewed papers (HNSW/Malkov 2018, Dong 2011, Kleinberg 2000) and industry-standard practices.

---

## 10. Implementation Phases

### Phase 1: Prototype (Week 1)

**Goal**: Validate mutual k-NN approach with minimal changes

**Deliverables**:
- Refactor `LinkingHandler::execute` to support strategy dispatch
- Implement mutual k-NN strategy (FR-2)
- Add adaptive k (FR-3) and fallback linking (FR-4)
- Test on development corpus (100 notes)

**Success Criteria**:
- Tests pass with topology improvements (clustering >0.3, avg degree 5-10)
- Latency <200ms per note

**Effort**: 8-16 hours

### Phase 2: Metrics & Monitoring (Week 2)

**Goal**: Enable visibility into topology quality

**Deliverables**:
- Implement topology stats endpoint (FR-5)
- Add latency instrumentation
- Document configuration options

**Success Criteria**:
- Endpoint returns accurate metrics for test corpus
- Response time <5s for 10k notes

**Effort**: 4-8 hours

### Phase 3: Configuration & Testing (Week 2-3)

**Goal**: Production-ready configuration management

**Deliverables**:
- Environment variable wiring (FR-1)
- Default threshold behavior preservation (FR-6)
- Integration tests for all strategies
- Documentation updates

**Success Criteria**:
- All tests pass with both strategies
- Existing deployments unaffected (threshold default)

**Effort**: 4-8 hours

### Phase 4: A/B Testing (Week 3-4)

**Goal**: Validate user experience improvements

**Deliverables**:
- Deploy to 20% of users with `GRAPH_LINKING_STRATEGY=mutual_knn`
- Track engagement metrics (depth>1 navigation)
- Gather qualitative feedback

**Success Criteria**:
- >30% of link clicks reach depth>1 (vs. <10% baseline)
- No increase in "missing link" support queries

**Effort**: 8-16 hours (deployment + analysis)

### Phase 5: Rollout or Rollback (Week 5)

**Decision Point**: Based on Phase 4 metrics

**If Success**:
- Update default to `mutual_knn`
- Publish blog post explaining topology improvements
- Deprecate threshold strategy in future release

**If Failure**:
- Revert to threshold default
- Document learnings for future iteration
- Investigate alternative approaches

---

## 11. Documentation Updates

### 11.1 Administrator Guide

**File**: `docs/admin-guide.md` (new section)

**Content**:
- Graph topology concepts (star vs. mesh)
- Configuration variables and their effects
- Topology metrics interpretation
- Troubleshooting isolated nodes

### 11.2 API Reference

**File**: `crates/matric-api/src/openapi.yaml`

**Content**:
- `GET /api/v1/graph/topology/stats` endpoint documentation
- Response schema for `TopologyStats`

### 11.3 Research Archive

**Files**: Already completed in `docs/research/`
- Link from PRD to research documents
- Executive summary for decision-makers

### 11.4 Changelog

**File**: `CHANGELOG.md`

**Content**:
```markdown
## [YYYY.M.PATCH] - YYYY-MM-DD

### Added
- Mutual k-NN linking strategy for improved graph topology
- `GET /api/v1/graph/topology/stats` endpoint for monitoring
- Adaptive k based on corpus size
- Isolated node fallback to prevent disconnected graphs

### Changed
- `GRAPH_LINKING_STRATEGY` environment variable now configurable
- Link metadata includes strategy provenance

### Deprecated
- Threshold-based linking (will remain as option through YYYY.M)
```

---

## 12. Testing Strategy

### 12.1 Unit Tests

**File**: `crates/matric-db/tests/linking.rs`

**Scenarios**:
- Mutual k-NN creates bidirectional links
- Each note has ≤k outgoing links
- Isolated node fallback activates correctly
- Adaptive k computes correct value for various corpus sizes

**Coverage Target**: >90% of new code paths

### 12.2 Integration Tests

**File**: `crates/matric-api/tests/graph_topology.rs`

**Scenarios**:
- Strategy configuration via environment variables
- Topology stats endpoint accuracy
- Dual-mode operation (threshold + k-NN in same deployment)

### 12.3 Topology Quality Tests

**File**: `crates/matric-api/tests/topology_metrics.rs`

**Scenarios**:
- Clustering coefficient improves (>0.3) with k-NN
- Degree distribution more uniform (lower std dev)
- Average path length increases (2.0 → 3-4)

**Method**: Create synthetic corpus, measure topology before/after k-NN

### 12.4 Performance Benchmarks

**File**: `crates/matric-api/benches/linking.rs` (new)

**Scenarios**:
- Linking latency for 100, 1k, 10k note corpora
- Comparison: threshold vs. k-NN latency
- Topology stats computation time

**Acceptance**: k-NN latency <200ms p95, stats computation <5s

---

## 13. Open Questions

### Q1: Multi-Memory Topology Isolation

**Question**: Should each memory archive maintain independent k values based on its corpus size, or use global configuration?

**Considerations**:
- Large memory (10k notes) may need k=15
- Small memory (100 notes) may need k=5
- Current architecture supports per-memory configuration via schema context

**Recommendation**: Start with global configuration; add per-memory override in future if heterogeneous corpus sizes observed.

### Q2: Link Metadata Schema Versioning

**Question**: Should we version the metadata JSON to support future schema evolution?

**Considerations**:
- Current: `{"strategy": "mutual_knn", "k": 7}`
- Future: `{"version": 2, "strategy": {...}, "quality_metrics": {...}}`

**Recommendation**: Add optional `metadata_version` field now, default to 1 if absent. Plan for backward-compatible reads.

### Q3: Topology Optimization Triggers

**Question**: Should we automatically trigger re-linking when topology metrics degrade below thresholds?

**Considerations**:
- Pro: Self-healing graph quality
- Con: Expensive batch operation, may disrupt user workflows

**Recommendation**: Defer to future enhancement. Manual trigger via admin API sufficient for MVP.

---

## 14. Acceptance Criteria Summary

Feature is complete when:

1. Mutual k-NN linking implemented and tested (FR-2)
2. Configuration via environment variables working (FR-1)
3. Topology metrics endpoint functional (FR-5)
4. Adaptive k and isolated node fallback operational (FR-3, FR-4)
5. Backward compatibility verified (FR-6)
6. Performance benchmarks meet targets (NFR-1)
7. A/B testing shows >30% depth>1 navigation improvement
8. Documentation complete (admin guide, API reference, changelog)
9. All tests pass with >90% coverage

---

## 15. Appendix: Research References

### Peer-Reviewed Papers

1. **Malkov & Yashunin (2018)** - "Efficient and robust approximate nearest neighbor search using Hierarchical Navigable Small World graphs" - Foundation for HNSW index used by pgvector (REF-031 in research corpus)

2. **Dong et al. (2011)** - "Efficient k-nearest neighbor graph construction for generic similarity measures" - Validates k-NN for knowledge graphs

3. **Kleinberg (2000)** - "The Small-World Phenomenon: An Algorithmic Perspective" - Theoretical basis for bounded-degree graphs

4. **Newman (2002)** - "Assortative Mixing in Networks" - Community structure in k-NN graphs

### Internal Research

- `/home/roctinam/dev/fortemi/docs/research/knowledge-graph-topology-techniques.md` - Comprehensive 7-technique analysis
- `/home/roctinam/dev/fortemi/docs/research/graph-topology-implementation-guide.md` - Production-ready code patterns
- `/home/roctinam/dev/fortemi/docs/research/graph-topology-executive-summary.md` - Decision rationale and roadmap

---

## 16. Approval Signatures

**Product Owner**: _________________ Date: _______

**Engineering Lead**: _________________ Date: _______

**QA Lead**: _________________ Date: _______

---

**Document Version**: 1.0
**Last Updated**: 2026-02-14
**Next Review**: Before Phase 4 (A/B testing decision)
