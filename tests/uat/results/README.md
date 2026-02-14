# UAT Test Results

This directory contains User Acceptance Test (UAT) results for the Matric Memory API.

## Test Runs

| Date | Run | Phases | Tests | Pass | Fail | Blocked | Pass Rate | Report |
|------|-----|--------|-------|------|------|---------|-----------|--------|
| 2026-02-08 | Run 1 | 7-9 | 31 | 21 | 8 | 2 | 67.7% | [Report](./uat-report-2026-02-08-phases-7-9.md) |
| 2026-02-07 | Run 2 | 0-21 | 447 | 389 | 15 | 43 | 87.0% | [Report](./uat-report-2026-02-07-v2.md) |
| 2026-02-07 | Run 1 | 0-19 | 530 | 425 | 30 | 75 | 80.2% | [Report](./uat-report-2026-02-07.md) |
| 2026-02-06 | Run 1 | 0-18 | 488 | 337 | 19 | 132 | 69.1% | [Report](./uat-report-2026-02-06.md) |

## Latest Results (2026-02-08)

### Phase 7: Embeddings
- **Status**: CRITICAL ISSUES
- **Pass Rate**: 60.0% (9/15)
- **Key Finding**: Embedding sets disappear after creation (Issue #186)

### Phase 8: Document Types
- **Status**: MOSTLY WORKING
- **Pass Rate**: 83.3% (5/6 executed)
- **Key Finding**: Missing display_name field documentation (Issue #188)

### Phase 9: Edge Cases
- **Status**: EXCELLENT
- **Pass Rate**: 100% (8/8)
- **Key Finding**: Robust security and edge case handling

## Critical Issues Discovered

| Issue | Title | Priority | Status |
|-------|-------|----------|--------|
| #186 | Embedding sets disappear after creation | High | Open |
| #188 | create_document_type requires undocumented display_name | High | Open |
| #187 | reembed_all endpoint returns empty response | Medium | Open |
| #189 | Empty content and very long tags not validated | Low | Open |

## Test Coverage

### Completed Phases (0-9)
- Phase 0: Setup and Health Checks
- Phase 1: Core CRUD Operations
- Phase 2: Tagging and Metadata
- Phase 3: Collections and Hierarchy
- Phase 4: Templates
- Phase 5: Graph Operations
- Phase 6: Attachments
- **Phase 7: Embeddings** (2026-02-08)
- **Phase 8: Document Types** (2026-02-08)
- **Phase 9: Edge Cases** (2026-02-08)

### Remaining Phases
- Phase 10-21: Advanced features (SKOS, versioning, temporal-spatial, PKE, etc.)

## Quick Links

- [Latest Summary](./uat-phases-7-9-summary-final.md)
- [Full Report](./uat-report-2026-02-08-phases-7-9.md)
- [GitHub Issues](https://github.com/fortemi/fortemi/issues)
- [API URL](https://localhost:3000)

## Test Methodology

All tests executed via direct API calls (curl) against production instance at https://localhost:3000. Tests follow isolation requirements to prevent data corruption and side effects.

**Test Standards**:
- Attempt all tests before marking BLOCKED
- Record all error messages verbatim
- File Gitea issues for all failures
- Provide reproduction steps for each bug
- Verify security (SQL injection, XSS)
- Test concurrent operations
- Test edge cases (empty, large, special chars)

---

**Last Updated**: 2026-02-08
**Maintained By**: Claude Code (Test Engineer)
