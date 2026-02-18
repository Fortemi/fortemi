# Discovery Report: Multilingual Full-Text Search Support

---
title: Multilingual FTS Discovery Report
version: 1.0
status: BASELINED
date: 2026-02-01
phase: Discovery
primary-author: Requirements Analyst
reviewers: [Architecture Designer, Technical Researcher]
synthesizer: Documentation Synthesizer
---

## 1. Executive Summary

### 1.1 Problem Statement

matric-memory's current full-text search implementation is limited to English-only content, preventing effective search for users working with multilingual content. Three critical issues have been identified:

| Issue | Description | Impact |
|-------|-------------|--------|
| **#316** | CJK (Chinese, Japanese, Korean) text search fails | Users cannot search content in CJK languages; single character searches return no results |
| **#319** | Emoji characters are not searchable | Modern knowledge organization using emoji as semantic markers is broken |
| **#308** | OR operator not supported | Power users cannot perform boolean searches ("cat OR dog") |

**Root Cause:** The current implementation uses `plainto_tsquery()` with the `matric_english` text search configuration, which:
- Strips CJK characters as non-alphanumeric noise
- Ignores emoji entirely
- Does not support boolean operators (OR, NOT, phrase search)

### 1.2 Proposed Solution

Implement a phased multilingual FTS solution using PostgreSQL's built-in capabilities and extensions:

| Phase | Solution | Complexity | Impact |
|-------|----------|------------|--------|
| **Phase 1** | Switch to `websearch_to_tsquery()` + `simple` config | Low | Fixes #308, improves #316 |
| **Phase 2** | Add pg_trgm trigram indexes | Low | Universal fallback for all scripts |
| **Phase 3** | Add pg_bigm bigram indexes | Medium | Production-grade CJK support |

### 1.3 Expected Impact

| Metric | Current | After Implementation |
|--------|---------|---------------------|
| Languages supported | English only | English, CJK, Cyrillic, Arabic, Hebrew, Emoji |
| Boolean operators | None | OR, NOT, phrase search, wildcards |
| Search latency (p95) | ~50ms | ~80-120ms (within 200ms target) |
| Index size | 1x baseline | 3-5x (acceptable for multi-config) |
| Issues resolved | 0 | 3 (#316, #319, #308) |

---

## 2. Requirements Summary

### 2.1 Must Have (Critical)

| ID | Requirement | User Story | Acceptance Criteria |
|----|-------------|------------|---------------------|
| **SR-001** | CJK Language Support | Search notes in Chinese, Japanese, Korean | Single character search returns results; mixed CJK+English queries work |
| **SR-006** | OR Operator Support | Boolean search with OR, NOT, phrase | "cat OR dog" returns notes containing either term |
| **NFR-001** | Performance | Search latency <200ms p95 | All query types complete within SLA |
| **NFR-003** | Backward Compatibility | Existing English searches unchanged | Default behavior remains `matric_english` |

### 2.2 Should Have (Important)

| ID | Requirement | User Story | Acceptance Criteria |
|----|-------------|------------|---------------------|
| **SR-002** | Emoji/Symbol Search | Search by emoji markers and Unicode symbols | "fire" emoji returns tagged notes |
| **SR-003** | Arabic Script Support | Search RTL languages (Arabic, Hebrew, Persian) | Word boundaries respected; diacritic handling |
| **SR-004** | Cyrillic Script Support | Search Russian, Ukrainian content | Case-insensitive; morphological variants match |
| **NFR-002** | Language Detection | Auto-detect query language | Fallback to `simple` config for unknown languages |

### 2.3 Could Have (Nice to Have)

| ID | Requirement | User Story | Acceptance Criteria |
|----|-------------|------------|---------------------|
| **SR-005** | Other Scripts | Thai, Vietnamese, Devanagari, Greek | Character-level search functional |
| **FR-8** | Cross-lingual Search | Query in English, find Chinese documents | Semantic search handles cross-lingual retrieval |

### 2.4 Requirements Traceability

```
Issue #316 (CJK fails) --> SR-001 (CJK Support) --> Phase 2/3 Implementation
Issue #319 (Emoji fails) --> SR-002 (Emoji Search) --> Phase 2 (pg_trgm)
Issue #308 (OR fails) --> SR-006 (OR Operators) --> Phase 1 (websearch_to_tsquery)
```

---

## 3. Technical Approach

### 3.1 Phase 1: Quick Wins (websearch_to_tsquery + simple config)

**Duration:** 1-2 days
**Risk Level:** Low
**Fixes:** Issue #308 (OR operators)

**Changes:**
1. Replace `plainto_tsquery()` with `websearch_to_tsquery()` throughout codebase
2. Create `matric_simple` text search configuration (no stemming)
3. Add GIN index on `simple` configuration for CJK fallback
4. Implement query language detection (Unicode script analysis)

**New Query Capabilities:**
```sql
-- OR operator (NEW)
websearch_to_tsquery('english', 'cat or dog')  --> 'cat' | 'dog'

-- NOT operator (NEW)
websearch_to_tsquery('english', 'cat -dog')    --> 'cat' & !'dog'

-- Phrase search (NEW)
websearch_to_tsquery('english', '"machine learning"')  --> 'machin' <-> 'learn'
```

**Code Changes:**
- `crates/matric-db/src/search.rs`: Replace all `plainto_tsquery` calls
- `crates/matric-db/src/skos_tags.rs`: Update tag search
- `crates/matric-db/src/embedding_sets.rs`: Update embedding search

**Migration:**
```sql
-- 20260202_phase1_websearch_simple.sql
CREATE TEXT SEARCH CONFIGURATION matric_simple (COPY = simple);

CREATE INDEX idx_note_revised_tsv_simple ON note_revised_current
  USING gin (to_tsvector('matric_simple', content));
```

### 3.2 Phase 2: Universal Fallback (pg_trgm trigram indexes)

**Duration:** 3-5 days
**Risk Level:** Low
**Fixes:** Issue #319 (Emoji search), improves #316 (CJK partial)

**Changes:**
1. Enable `pg_trgm` extension (built-in, no compilation)
2. Create GIN trigram indexes on content and title columns
3. Implement trigram-based similarity search
4. Add fallback routing for emoji and unknown scripts

**Capabilities Added:**
- Emoji exact match via trigram index
- Fuzzy/typo-tolerant search
- Partial word matching ("prog" finds "programming")
- Universal script support (all Unicode)

**Migration:**
```sql
-- 20260205_phase2_trigram.sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX idx_note_revised_trgm ON note_revised_current
  USING gin (content gin_trgm_ops);

CREATE INDEX idx_note_title_trgm ON note
  USING gin (title gin_trgm_ops);
```

**Query Routing Logic:**
```rust
fn select_search_strategy(query: &str) -> SearchStrategy {
    if has_emoji(query) {
        SearchStrategy::Trigram
    } else if has_cjk(query) {
        SearchStrategy::Simple  // Phase 2: Simple, Phase 3: Bigram
    } else {
        SearchStrategy::English
    }
}
```

### 3.3 Phase 3: Production CJK (pg_bigm bigram indexes)

**Duration:** 5-10 days
**Risk Level:** Medium
**Fixes:** Issue #316 (CJK production-grade)

**Changes:**
1. Compile and install `pg_bigm` extension
2. Configure `shared_preload_libraries` (requires restart)
3. Create GIN bigram indexes for CJK-optimized search
4. Implement hybrid query routing (FTS + bigram)

**Why pg_bigm over pg_trgm for CJK:**

| Feature | pg_trgm (3-gram) | pg_bigm (2-gram) |
|---------|------------------|------------------|
| Short keyword (1-2 chars) | Slow (seq scan) | Fast (indexed) |
| CJK support | Limited | Excellent |
| Memory efficiency | Lower | Higher for CJK |

**Migration:**
```sql
-- 20260210_phase3_bigram.sql
-- Requires: pg_bigm extension installed, shared_preload_libraries configured

CREATE EXTENSION IF NOT EXISTS pg_bigm;

CREATE INDEX idx_note_revised_bigm ON note_revised_current
  USING gin (content gin_bigm_ops);

CREATE INDEX idx_note_title_bigm ON note
  USING gin (title gin_bigm_ops);
```

**Graceful Degradation:**
```rust
// If pg_bigm unavailable, fall back to pg_trgm
async fn search_cjk(&self, query: &str) -> Result<Vec<SearchHit>> {
    if self.has_bigm_extension().await? {
        self.search_bigm(query).await
    } else {
        self.search_trgm(query).await  // Fallback
    }
}
```

---

## 4. Architecture Changes

### 4.1 New Database Objects

**Text Search Configurations:**
```sql
matric_english  -- Existing (unchanged)
matric_simple   -- NEW: No stemming, universal tokenization
matric_german   -- NEW: German compound word splitting
matric_russian  -- NEW: Cyrillic stemming
```

**New Indexes:**
```sql
idx_note_revised_tsv_simple  -- GIN on simple tsvector
idx_note_revised_trgm        -- GIN trigram for fuzzy/emoji
idx_note_title_trgm          -- GIN trigram on titles
idx_note_revised_bigm        -- GIN bigram for CJK (optional)
idx_note_title_bigm          -- GIN bigram on titles (optional)
```

**New Columns (optional):**
```sql
note.detected_language       -- ISO 639-1 code (auto-detected)
note.language_confidence     -- Detection confidence 0.0-1.0
```

### 4.2 Component Architecture

```
                    Search Request (query, lang_hint?)
                              |
                              v
                   +---------------------+
                   | Query Preprocessor  |
                   | - Script detection  |
                   | - Language hints    |
                   +---------------------+
                              |
                              v
                   +---------------------+
                   | Strategy Selector   |
                   | - Latin -> English  |
                   | - CJK -> Bigram     |
                   | - Emoji -> Trigram  |
                   | - Mixed -> Multi    |
                   +---------------------+
                              |
              +---------------+---------------+
              |                               |
              v                               v
     +----------------+              +----------------+
     | FTS Branch     |              | Semantic Branch|
     | (multi-config) |              | (unchanged)    |
     +----------------+              +----------------+
              |                               |
              +---------------+---------------+
                              |
                              v
                   +---------------------+
                   | RRF Fusion          |
                   | (adaptive k by      |
                   |  script type)       |
                   +---------------------+
                              |
                              v
                      Search Results
```

### 4.3 API Parameter Additions

**New Optional Parameters:**
```yaml
GET /api/v1/search
  - q: string (required)
  - limit: integer (default: 20)
  - lang: string (optional, ISO 639-1 code)
  - script: string (optional, latin|han|cyrillic|hangul|arabic)
```

**New Response Metadata:**
```json
{
  "results": [...],
  "metadata": {
    "detected_language": "zh",
    "search_strategy": "bigram",
    "fts_hits": 15,
    "semantic_hits": 30
  }
}
```

### 4.4 Migration Strategy

**Zero-Downtime Migration:**

| Phase | Action | Blocking | Duration |
|-------|--------|----------|----------|
| 1 | Enable extensions | No | <1 min |
| 2 | Create text configs | No | <1 min |
| 3 | Create indexes (CONCURRENTLY) | No | 5-30 min |
| 4 | Deploy code changes | No | <1 min |
| 5 | Feature flag rollout | No | 1-7 days |

**Rollback Plan:**
```sql
-- Disable feature flag
-- DROP INDEX IF EXISTS idx_note_revised_trgm;
-- DROP INDEX IF EXISTS idx_note_revised_bigm;
-- Application falls back to matric_english automatically
```

---

## 5. Risk Assessment

### 5.1 Risk Matrix

| Risk | Likelihood | Impact | Severity | Mitigation |
|------|------------|--------|----------|------------|
| Index size growth 3-5x | High | Medium | Medium | Monitor storage; acceptable for feature value |
| Query latency +10-40% | Medium | Medium | Medium | Feature flag; A/B testing; tune thresholds |
| pg_bigm unavailable in cloud | Medium | High | High | Graceful fallback to pg_trgm |
| Language detection errors | Medium | Low | Low | Manual override; semantic search fallback |
| Migration blocks writes | Low | High | Medium | CREATE INDEX CONCURRENTLY |
| Query parser complexity | Medium | Medium | Medium | Comprehensive test suite; fuzzing |

### 5.2 Risk Mitigation Details

**Index Size Growth (3-5x):**
- **Mitigation:** Monitoring and alerting on disk usage
- **Acceptance:** Acceptable trade-off for multilingual capability
- **Fallback:** Drop bigram indexes if critical; trigram sufficient

**Query Latency Impact (+10-40%):**
- **Current SLA:** <200ms p95
- **Projected:** 80-120ms (within SLA)
- **Mitigation:** Feature flag for gradual rollout; per-script metrics

**pg_bigm Extension Unavailable:**
- **Affected:** Cloud hosting (RDS, Cloud SQL)
- **Mitigation:** pg_trgm fallback provides 80% of CJK functionality
- **Code:** Conditional extension detection at runtime

**Rollback Strategy:**
1. Disable feature flag (immediate)
2. Drop new indexes (optional, storage recovery)
3. No data migration required
4. Original `matric_english` index unchanged

---

## 6. Implementation Roadmap

### 6.1 Phase Timeline

```
Week 1-2: Phase 1 (Quick Wins)
+------------------------------------------+
| Day 1-2: websearch_to_tsquery migration  |
|   - Code changes in matric-db            |
|   - Unit tests for new query syntax      |
|   - Integration tests                    |
+------------------------------------------+
| Day 3-4: matric_simple configuration     |
|   - Database migration                   |
|   - Language detection function          |
|   - Query routing logic                  |
+------------------------------------------+
| Day 5: Testing and deployment            |
|   - Performance benchmarks               |
|   - Staging deployment                   |
|   - Production rollout                   |
+------------------------------------------+

Week 3-4: Phase 2 (Trigram Indexes)
+------------------------------------------+
| Day 1-2: pg_trgm extension setup         |
|   - Migration script                     |
|   - Index creation (CONCURRENTLY)        |
+------------------------------------------+
| Day 3-5: Trigram search implementation   |
|   - Similarity search functions          |
|   - Emoji detection and routing          |
|   - Fuzzy match integration              |
+------------------------------------------+
| Day 6-7: Testing and rollout             |
|   - Emoji search tests                   |
|   - Performance benchmarks               |
|   - Gradual production rollout           |
+------------------------------------------+

Week 5-6: Phase 3 (CJK Production)
+------------------------------------------+
| Day 1-3: pg_bigm extension               |
|   - Docker image with pg_bigm            |
|   - Extension compilation                |
|   - Configuration changes                |
+------------------------------------------+
| Day 4-7: Bigram search implementation    |
|   - CJK-optimized search functions       |
|   - Query routing refinement             |
|   - Graceful degradation logic           |
+------------------------------------------+
| Day 8-10: Testing and production         |
|   - CJK test fixtures                    |
|   - Native speaker validation            |
|   - Production deployment                |
+------------------------------------------+
```

### 6.2 Effort Estimates

| Phase | Tasks | Effort | Dependencies |
|-------|-------|--------|--------------|
| Phase 1 | websearch_to_tsquery + simple config | 1-2 days | None |
| Phase 2 | pg_trgm trigram indexes | 3-5 days | Phase 1 |
| Phase 3 | pg_bigm bigram indexes | 5-10 days | Phase 2, Docker build |
| **Total** | All phases | 9-17 days | Sequential |

### 6.3 Team Requirements

- 1 Backend Engineer (PostgreSQL/Rust expertise)
- 0.5 QA Engineer (i18n testing)
- 0.25 DevOps (Docker, extension management)

---

## 7. Success Metrics

### 7.1 Functional Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Issue #316 resolved | CJK search returns results | Integration test passes |
| Issue #319 resolved | Emoji search returns results | Integration test passes |
| Issue #308 resolved | OR operator works | Integration test passes |
| Language detection accuracy | >90% | Validation against test corpus |
| Zero English regressions | 100% existing tests pass | CI pipeline |

### 7.2 Performance Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Search latency p95 | <200ms | Prometheus/Grafana |
| Index size growth | <5x baseline | PostgreSQL `pg_relation_size()` |
| Query throughput | >100 QPS | Load testing |
| Language detection overhead | <10ms | Instrumentation |

### 7.3 Quality Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Test coverage (new code) | >90% | cargo tarpaulin |
| CJK false positive rate | <5% | Precision testing |
| Emoji exact match rate | 100% | Integration tests |
| Documentation completeness | 100% | Docs review |

### 7.4 Issue Resolution Verification

| Issue | Test Case | Expected Result |
|-------|-----------|-----------------|
| #316 | Search for single CJK character | Returns matching notes |
| #316 | Search for CJK phrase | Returns exact matches |
| #319 | Search for fire emoji | Returns emoji-tagged notes |
| #308 | Search "cat OR dog" | Returns notes with either term |
| #308 | Search "cat -dog" | Excludes notes with "dog" |
| #308 | Search "machine learning" (phrase) | Returns exact phrase matches |

---

## 8. Handoff Checklist

### 8.1 Definition of Ready (DoR) Validation

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Problem clearly defined | PASS | Issues #316, #319, #308 documented |
| Stakeholder requirements captured | PASS | SR-001 through SR-006 |
| Technical approach validated | PASS | Technical research spike complete |
| Architecture designed | PASS | Architecture design document |
| Risks identified and mitigated | PASS | Risk matrix with mitigations |
| Effort estimated | PASS | 9-17 days across 3 phases |
| Success metrics defined | PASS | Functional, performance, quality metrics |
| Rollback plan documented | PASS | Zero-downtime rollback strategy |

### 8.2 Architectural Decision Records (ADRs)

| ADR ID | Title | Status |
|--------|-------|--------|
| ADR-ML-001 | Script Detection Approach | Proposed |
| ADR-ML-002 | CJK Search Strategy (pg_bigm) | Proposed |
| ADR-ML-003 | Multi-Index Strategy | Proposed |
| ADR-ML-004 | Backward Compatibility | Proposed |

**ADRs require formal approval before implementation begins.**

### 8.3 Issues to Create

| Issue Title | Type | Priority | Phase |
|-------------|------|----------|-------|
| Switch plainto_tsquery to websearch_to_tsquery | Enhancement | High | 1 |
| Create matric_simple text search configuration | Enhancement | High | 1 |
| Add language detection to search queries | Enhancement | Medium | 1 |
| Enable pg_trgm and create trigram indexes | Enhancement | High | 2 |
| Implement trigram similarity search | Enhancement | Medium | 2 |
| Add emoji search support | Enhancement | Medium | 2 |
| Compile and install pg_bigm extension | Enhancement | Medium | 3 |
| Implement bigram search for CJK | Enhancement | Medium | 3 |
| Add lang/script API parameters | Enhancement | Low | 3 |

### 8.4 Documentation Updates Required

| Document | Update Required |
|----------|-----------------|
| CLAUDE.md | Add multilingual search syntax examples |
| API docs | Document new lang/script parameters |
| CHANGELOG.md | Add feature announcement |
| MCP docs | Update search tool documentation |

---

## 9. Sign-Off

### 9.1 Required Approvals

| Role | Name | Status | Date |
|------|------|--------|------|
| Requirements Analyst | (Primary Author) | APPROVED | 2026-02-01 |
| Architecture Designer | (Reviewer) | APPROVED | 2026-02-01 |
| Technical Researcher | (Reviewer) | APPROVED | 2026-02-01 |
| Product Owner | (Pending) | PENDING | - |

### 9.2 Conditions for Approval

1. **Product Owner Approval:** Confirm phased rollout approach acceptable
2. **Performance SLA Confirmation:** Accept 10-40% latency increase for multilingual support
3. **Extension Policy:** Approve pg_bigm dependency (or confirm pg_trgm-only fallback acceptable)

### 9.3 Outstanding Concerns

| Concern | Raised By | Severity | Resolution |
|---------|-----------|----------|------------|
| pg_bigm unavailable in cloud hosting | Architecture Designer | MEDIUM | Document pg_trgm fallback; test in target environments |
| Index size growth exceeds 5x | Technical Researcher | LOW | Monitor; document cleanup procedure if needed |

---

## 10. Appendices

### 10.1 Related Issues

- Issue #316: Single CJK character search fails
- Issue #319: Emoji not searchable
- Issue #308: OR operator not supported

### 10.2 Source Documents

| Document | Location | Author |
|----------|----------|--------|
| Stakeholder Requirements | `.aiwg/working/discovery/multilingual-fts/requests/stakeholder-requests.md` | Requirements Analyst |
| Technical Research | `.aiwg/working/discovery/multilingual-fts/spikes/technical-research.md` | Technical Researcher |
| Architecture Design | `.aiwg/working/discovery/multilingual-fts/designs/architecture-design.md` | Architecture Designer |

### 10.3 Technical References

- [PostgreSQL Text Search](https://www.postgresql.org/docs/16/textsearch.html)
- [pg_trgm Extension](https://www.postgresql.org/docs/16/pgtrgm.html)
- [pg_bigm Documentation](https://github.com/pgbigm/pg_bigm)
- [websearch_to_tsquery](https://www.postgresql.org/docs/16/textsearch-controls.html#TEXTSEARCH-PARSING-QUERIES)

### 10.4 Unicode Script Ranges Reference

```
Latin:       U+0041-U+007A, U+00C0-U+02AF
Han (CJK):   U+4E00-U+9FFF, U+3400-U+4DBF
Hiragana:    U+3040-U+309F
Katakana:    U+30A0-U+30FF
Hangul:      U+AC00-U+D7AF
Cyrillic:    U+0400-U+04FF
Arabic:      U+0600-U+06FF
Hebrew:      U+0590-U+05FF
Emoji:       U+1F300-U+1F9FF
```

---

**Document Version:** 1.0
**Synthesis Date:** 2026-02-01
**Synthesizer:** Documentation Synthesizer
**Status:** BASELINED pending Product Owner approval
