# Gap Analysis - matric-memory Open Issues

**Date**: 2026-01-26
**Total Open Issues**: 20
**Issues Closed This Session**: 4 (#166, #171, #161 + earlier #179-184, #155-157)

---

## Executive Summary

| Category | Count | Status |
|----------|-------|--------|
| CRITICAL | 1 | Needs implementation |
| HIGH | 4 | Ready for development |
| MEDIUM | 4 | Queued for future sprints |
| LOW | 5 | Future enhancements |
| Documentation | 4 | Partially complete |
| Deferred | 2 | Appropriately deferred |

---

## 1. CRITICAL Priority (1 issue)

### #162: W3C PROV Provenance Tracking for AI Revisions

**Gap Analysis:**
- **Schema**: `provenance_edge` table defined in migration but not deployed to production
- **Rust Models**: `ProvenanceEdge` struct exists in `matric-core/src/models.rs:692`
- **API**: No endpoints for provenance queries
- **Integration**: Not connected to AI revision pipeline

**What Exists:**
```rust
// crates/matric-core/src/models.rs:690
pub struct ProvenanceEdge {
    // ...defined but not used
}
```

**Gap:**
- [ ] Deploy provenance_edge table to production
- [ ] Implement ProvenanceRepository trait
- [ ] Create API endpoints for provenance queries
- [ ] Integrate with AI revision handlers to record provenance

**Estimated Effort**: 1-2 weeks

---

## 2. HIGH Priority (4 issues)

### #167: Migrate Embeddings from nomic-embed-text to E5

**Gap Analysis:**
- **Current**: Using nomic-embed-text (768-dim)
- **Target**: E5 model family for better retrieval quality
- **Migration**: Requires re-embedding all notes

**What Exists:**
- Embedding infrastructure fully operational
- Background job framework for batch processing

**Gap:**
- [ ] Verify E5 availability in Ollama
- [ ] Create embedding migration job
- [ ] Handle dimension compatibility
- [ ] A/B testing infrastructure

**Estimated Effort**: 1 week + re-embedding time

---

### #165: Reflexion Self-Improvement via Episodic Memory

**Gap Analysis:**
- **Current**: One-shot AI revision without learning
- **Target**: Learn from past interactions to improve

**What Exists:**
- AI revision pipeline
- Provenance edge concept (schema exists)

**Gap:**
- [ ] Design `ai_episodes` table schema
- [ ] Implement episode storage on revision feedback
- [ ] Add episodic retrieval to revision context
- [ ] Quality scoring mechanism

**Estimated Effort**: 2-3 weeks

---

### #164: ReAct Agent Pattern for Transparent AI Reasoning

**Gap Analysis:**
- **Current**: Black-box AI generation
- **Target**: Visible thought→action→observation loops

**What Exists:**
- Inference crate with Ollama integration
- Job handlers for AI tasks

**Gap:**
- [ ] Define ReAct prompt templates
- [ ] Create reasoning trace storage
- [ ] API to expose reasoning steps
- [ ] UI integration (if applicable)

**Estimated Effort**: 2 weeks

---

### #163: Self-Refine Iterative AI Revision

**Gap Analysis:**
- **Current**: Single-pass AI revision
- **Target**: Iterative self-critique and refinement

**What Exists:**
- AI revision handlers
- Ollama generation infrastructure

**Gap:**
- [ ] Self-critique prompt design
- [ ] Quality threshold configuration
- [ ] Iteration loop implementation
- [ ] Token budget management

**Estimated Effort**: 1-2 weeks

---

## 3. MEDIUM Priority (4 issues)

### #172: Few-Shot Prompt Examples

**Gap**: No example storage or dynamic selection
**Effort**: 1 week

### #170: FAIR Metadata Export

**Gap**: No FAIR-compliant export endpoint
**Effort**: 3-5 days

### #169: BM25F Field-Weighted Scoring

**Gap**: ts_rank not configured with field weights
**Effort**: 2-3 days

### #168: Miller's Law Context Limits (7±2 Notes)

**Gap**: Context retrieval not limited
**Effort**: 1-2 days

---

## 4. LOW Priority (5 issues)

| # | Issue | Gap | Blocked By |
|---|-------|-----|------------|
| #177 | Dynamic ef_search Tuning | API parameter not exposed | - |
| #176 | Adaptive RRF k Parameter | Fixed k=60, no learning | - |
| #175 | SKOS Collections/Schemes | Basic tags only | - |
| #174 | Semantic Link Classification | Links untyped | - |
| #173 | ColBERT Re-ranking | Requires model deployment | - |

**Recommendation**: Defer until HIGH priority items complete.

---

## 5. Documentation (4 issues)

### #154: [Epic] Documentation Professionalization

**Progress**: ~75% complete
- ✅ Research acquisition (#155) - closed
- ✅ Terminology mapping (#156) - closed
- ✅ Academic citations (#157) - closed
- ✅ Research background (#161) - closed
- 🔄 README restructuring (#158) - partial
- 🔄 Multi-audience docs (#159) - partial
- ⏳ Marketing copy (#160) - not started

**Gap for #158 (README):**
- README has audience navigation but lacks complete quick-start guides

**Gap for #159 (Multi-Audience):**
- Developer docs exist, researcher docs exist
- Missing explicit "For Operators" guide

**Gap for #160 (Marketing Copy):**
- Not yet started
- Depends on #158, #159 completion

---

## 6. Deferred (2 issues)

### #61: Redis Caching Layer
**Status**: Appropriately deferred
**Trigger**: When query latency >100ms consistently

### #63: Tiered Storage (hot/warm/cold)
**Status**: Appropriately deferred
**Trigger**: When corpus >1M notes or storage costs significant

---

## Recommended Priority Order

### Immediate (Next Sprint)
1. **#162** - PROV tracking (CRITICAL - enables #165)
2. **#163** - Self-Refine (HIGH - quick win)
3. **#168** - Miller's Law limits (MEDIUM - 1-2 days)

### Short-term (Following Sprints)
4. **#164** - ReAct pattern
5. **#165** - Reflexion (depends on #162)
6. **#167** - E5 migration
7. **#169** - BM25F scoring

### Medium-term
8. **#172** - Few-shot examples
9. **#170** - FAIR export
10. Documentation completion (#158, #159, #160)

### Backlog
- All LOW priority issues (#173-177)
- Deferred issues (#61, #63)

---

## Implementation Dependencies

```
#162 (PROV) ──────┬──> #165 (Reflexion)
                  │
#163 (Self-Refine)┼──> #164 (ReAct) ──> Better AI Revision
                  │
#167 (E5) ────────┴──> #177 (ef_search) ──> #176 (RRF k)
                       requires HNSW ✅

#158 (README) ──> #159 (Multi-Audience) ──> #160 (Marketing) ──> #154 (Epic)
```

---

## Summary

**Closed This Session**: 4 issues
- #166 - HNSW migration (was already done)
- #171 - Soft delete (was already done)
- #161 - Research background doc (was already done)
- (Earlier: #179-184, #155-157)

**Remaining**: 20 open issues
- 1 CRITICAL (#162 PROV)
- 4 HIGH (#163-165, #167)
- 4 MEDIUM (#168-170, #172)
- 5 LOW (#173-177)
- 4 Documentation (#154, #158-160)
- 2 Deferred (#61, #63)

**Key Insight**: Several "open" issues were already implemented (#166, #171, #161). Recommend regular codebase audits to close implemented features.
