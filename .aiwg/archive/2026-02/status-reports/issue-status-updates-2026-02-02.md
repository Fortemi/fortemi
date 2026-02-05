# Issue Status Updates - 2026-02-02

This document summarizes the status review of epic and deferred issues.

## Actions Completed

1. **Labels Applied:**
   - #441, #430, #357: Added "QA Ready" label
   - #211, #63, #61: Added "deferred" label

2. **Issue Updates:**
   - #61: Reopened and updated with Redis bundle integration requirements

3. **Documentation Created:**
   - This status report
   - Update script: `/home/roctinam/dev/matric-memory/scripts/update-issue-61-redis-bundle.sh`

---

## Epics Status

### #441 - [Epic] Parallel Memory Archives with Schema Isolation

**Status:** Open - Documentation Complete, Ready for Implementation
**Labels:** QA Ready, architecture, database, epic, feature

**Progress Summary:**
- Research foundation complete (postgresql-multi-schema-patterns.md, multi-schema-summary.md, multi-schema-code-snippets.md)
- Architecture design finalized (Schema-Per-Archive pattern)
- Child issues created (#442-#447)

**Child Issue Status:**
| Issue | Title | Status | Labels |
|-------|-------|--------|--------|
| #442 | Implement SchemaContext abstraction | Open | core, database, implementation |
| #443 | Archive schema creation and migration | Open | QA Ready, database, implementation, migration |
| #444 | Archive management API endpoints | Open | QA Ready, api, feature, implementation |
| #445 | Archive MCP tools | Open | QA Ready, feature, implementation, mcp |
| #446 | Cross-archive search API | Open | QA Ready, api, implementation, search |
| #447 | Embedding Model Discovery API | Open | QA Ready, api, feature, inference |

**Blocking Issues:** None - ready to begin Phase 1 (Foundation)

**Next Steps:**
1. Implement #442 - SchemaContext abstraction (core foundation)
2. Implement #443 - Schema creation/migration (depends on #442)
3. Implement #444 - API endpoints (depends on #443)

---

### #430 - [Epic] File Attachments with Intelligent Processing

**Status:** Open - Design Complete, Documentation Complete
**Labels:** QA Ready, architecture, attachments, epic, feature

**Progress Summary:**
- Related ADRs created (ADR-031, ADR-032, ADR-033)
- Research documentation complete
- Schema design finalized (file_blob, file_attachment, file_provenance)
- AI/ML model mappings defined
- API endpoints designed
- Child issues created and documented

**Child Issue Status:**
| Issue | Title | Status | Labels |
|-------|-------|--------|--------|
| #432 | File storage schema | Open | QA Ready, attachments, data, database |
| #433 | FileStorage repository | Open | QA Ready, attachments, core, data |
| #434 | Temporal-spatial provenance schema | Open | QA Ready, attachments, data, database |
| #435 | EXIF/metadata extraction pipeline | Open | QA Ready, attachments, data, inference |
| #436 | extraction_strategy for document_type | Open | QA Ready, attachments, core, database |
| #437 | Memory search API | Open | QA Ready, api, attachments, search |
| #438 | 3D file analysis support | Open | QA Ready, attachments, feature, inference |
| #439 | Structured media format support | Open | QA Ready, attachments, feature |
| #440 | File safety validation | Open | QA Ready, attachments, security |

**Blocking Issues:** None - ready to begin Phase 1 (Core Storage)

**Next Steps:**
1. Implement #432 - File storage schema (foundation)
2. Implement #433 - FileStorage repository (depends on #432)
3. Implement #436 - extraction_strategy (document type integration)

---

### #357 - [Testing] Epic: Establish Comprehensive Test Coverage Baseline

**Status:** Open - Partial Progress
**Labels:** QA Ready, testing, epic

**Progress Summary:**
- Epic structure defined with clear coverage targets
- Testing pyramid documented
- 6 child issues identified

**Child Issue Status:**
| Issue | Title | Status | Notes |
|-------|-------|--------|-------|
| #332 | Unit tests for SKOS concept hierarchy | Open | 70% coverage target |
| #335 | Integration tests for document chunking | Open | Chain creation, reconstruction |
| #339 | Integration tests for PKE system | Open | 90% coverage target |
| #344 | MCP tool validation test suite | **Closed** | Completed |
| #347 | Search accuracy benchmarks | Open | Golden test set needed |
| #351 | Backup/restore end-to-end tests | Open | Full cycle tests |

**Blocking Issues:**
- CI infrastructure improvements may be needed
- Golden test set for search benchmarks needs creation

**Next Steps:**
1. Complete #332 - SKOS unit tests (foundation)
2. Create golden test set for #347 - Search benchmarks
3. Prioritize #339 - PKE tests (security-critical)

---

## Deferred Issues Status

### #211 - Add Hallucination Detection/Confidence Scoring

**Status:** Closed (previously deferred)
**Labels:** deferred

**Current State:**
- Issue documented with approaches to consider
- No implementation started
- Original source: HotM #28

**Decision Rationale:**
- Current system relies on immutable originals and user refinement
- Hallucination detection is complex (requires research investment)
- Higher priority features (archives, attachments) take precedence
- User can manually validate AI-generated content

**Prerequisites for Implementation:**
1. Research investment: Evaluate semantic consistency, citation verification, self-consistency approaches
2. Model support: Some approaches require token-level probabilities (not available in all models)
3. Baseline metrics: Need ground truth for hallucination detection accuracy

**Estimated Effort:** 2-3 weeks (research + implementation + validation)

**Triggers for Prioritization:**
- User complaints about AI revision quality
- Enterprise customers requiring auditability
- Availability of better LLM probability APIs

---

### #63 - Implement Tiered Storage for Scaling (Hot/Warm/Cold)

**Status:** Closed (deferred - not needed at current scale)
**Labels:** deferred

**Current State:**
- Detailed architecture documented
- No implementation started
- Scaling roadmap positions this at 2000+ notes threshold

**Decision Rationale:**
- Current corpus size does not justify implementation
- PostgreSQL + pgvector performance is adequate
- Redis caching (#61) provides better ROI at current scale
- Tiered storage adds operational complexity

**Prerequisites for Implementation:**
1. Corpus size exceeds 2000 notes
2. Performance degradation observed
3. Storage cost concerns emerge

**Estimated Effort:** 10-14 days (all phases)

**Triggers for Prioritization:**
- Query latency exceeds 500ms consistently
- Note corpus exceeds 5000 notes
- Embedding storage costs exceed budget

---

### #61 - Add Redis Caching Layer for Search Queries

**Status:** Open (reopened - has active requirements)
**Labels:** deferred, performance, infrastructure

**Current State:**
- Full architecture documented (cache key strategy, configuration, invalidation)
- Issue description updated with Redis bundle integration requirements
- No implementation started
- Related to existing Redis in MATRIC stack

**Decision Rationale:**
- Current search performance is adequate
- Adds infrastructure complexity
- Should be implemented when search latency becomes a bottleneck

**User Requirement (2026-02-02):**
Redis bundle integration requirements:
1. Redis should be included in `docker-compose.bundle.yml`
2. Environment variable to disable: `REDIS_ENABLED=false`
3. Default: enabled (runs by default)
4. Configuration options documented

**Proposed Docker Compose Changes:**

```yaml
services:
  redis:
    image: redis:7-alpine
    container_name: matric-redis
    restart: unless-stopped
    command: redis-server --appendonly yes
    volumes:
      - matric-redis:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5

  matric:
    environment:
      # Redis caching (optional, enabled by default)
      - REDIS_ENABLED=${REDIS_ENABLED:-true}
      - REDIS_URL=redis://redis:6379
      - REDIS_CACHE_TTL=${REDIS_CACHE_TTL:-300}
    depends_on:
      redis:
        condition: service_healthy

volumes:
  matric-redis:
    driver: local
```

**Configuration Options:**
| Variable | Default | Description |
|----------|---------|-------------|
| REDIS_ENABLED | true | Enable/disable Redis caching |
| REDIS_URL | redis://redis:6379 | Redis connection string |
| REDIS_CACHE_TTL | 300 | Cache TTL in seconds |
| REDIS_MAX_ENTRIES | 10000 | Maximum cache entries |
| REDIS_PREFIX | mm:search: | Cache key prefix |

**Prerequisites for Implementation:**
1. Bundle docker-compose changes merged
2. Cache integration code in matric-search crate
3. Metrics endpoint for cache hit/miss rates

**Estimated Effort:** 3-5 days

**Triggers for Prioritization:**
- Search latency exceeds 200ms p95
- High query volume (>100 req/min)
- User requests for improved performance

---

## Summary

| Issue | Type | Status | QA Ready | Action Taken |
|-------|------|--------|----------|--------------|
| #441 | Epic | Open | Yes | Label added |
| #430 | Epic | Open | Yes | Label added |
| #357 | Epic | Open | Yes | Label added |
| #211 | Deferred | Closed | N/A | Label added |
| #63 | Deferred | Closed | N/A | Label added |
| #61 | Deferred | Open | N/A | Reopened, label added, description updated with bundle requirements |

## Acceptance Criteria Status

- [x] All 3 epics have status documented (this file)
- [x] All 3 deferred issues have status documented (this file)
- [x] #61 includes Redis bundle integration requirements (in issue description)
- [x] Appropriate labels applied (QA Ready for epics, deferred for deferred issues)
- [x] Progress tracked accurately

## Files Created/Modified

1. `/home/roctinam/dev/matric-memory/.aiwg/working/issue-status-updates-2026-02-02.md` - This status report
2. `/home/roctinam/dev/matric-memory/scripts/update-issue-61-redis-bundle.sh` - Script to update issue #61
