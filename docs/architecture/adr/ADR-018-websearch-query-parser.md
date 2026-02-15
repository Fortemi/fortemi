# ADR-018: Query Parser Migration to websearch_to_tsquery

**Status:** Accepted (Implemented 2026-02-01)
**Date:** 2026-02-01
**Decision Makers:** @roctinam
**Technical Story:** Enable OR operators, NOT operators, and phrase search in FTS queries

## Context

Matric Memory currently uses PostgreSQL's `plainto_tsquery()` function to parse user search queries:

```sql
-- Current implementation
WHERE nrc.tsv @@ plainto_tsquery('matric_english', $1)
```

### Current Limitations (plainto_tsquery)

| Feature | Supported | Example |
|---------|-----------|---------|
| Simple keywords | Yes | "machine learning" |
| Implicit AND | Yes | "cat dog" → 'cat' & 'dog' |
| OR operator | No | "cat OR dog" → 'cat' & 'dog' (OR treated as keyword) |
| NOT operator | No | "cat -dog" → 'cat' & 'dog' (- stripped) |
| Phrase search | No | "\"machine learning\"" → 'machin' & 'learn' (quotes stripped) |
| Error tolerance | High | Garbage input returns empty/degraded query |

### Problem Statement

Users cannot express complex search queries:
- Issue #308: "cat OR dog" does not return notes containing either word
- No exclusion: Cannot search "python -java" to exclude Java results
- No phrase matching: Cannot search for exact phrase "machine learning"

These are standard search features expected by users familiar with Google-style search syntax.

## Decision

Migrate from `plainto_tsquery()` to `websearch_to_tsquery()` for all user-facing search queries.

### websearch_to_tsquery Capabilities

| Feature | Supported | Example | Result |
|---------|-----------|---------|--------|
| Simple keywords | Yes | "machine learning" | 'machin' & 'learn' |
| Implicit AND | Yes | "cat dog" | 'cat' & 'dog' |
| **OR operator** | **Yes** | "cat or dog" | 'cat' \| 'dog' |
| **NOT operator** | **Yes** | "cat -dog" | 'cat' & !'dog' |
| **Phrase search** | **Yes** | "\"machine learning\"" | 'machin' <-> 'learn' |
| Error tolerance | **Very High** | Garbage input returns safe query |

### Query Syntax Examples

```sql
-- OR operator (new capability)
SELECT websearch_to_tsquery('english', 'cat or dog');
-- Result: 'cat' | 'dog'

-- NOT operator (new capability)
SELECT websearch_to_tsquery('english', 'cat -dog');
-- Result: 'cat' & !'dog'

-- Phrase search (new capability)
SELECT websearch_to_tsquery('english', '"machine learning"');
-- Result: 'machin' <-> 'learn'

-- Combined operators
SELECT websearch_to_tsquery('english', '"deep learning" or "machine learning" -tensorflow');
-- Result: ('deep' <-> 'learn') | ('machin' <-> 'learn') & !'tensorflow'

-- Garbage input (safe handling)
SELECT websearch_to_tsquery('english', '""" )( dummy \\\\ query <->');
-- Result: 'dummi' & 'queri'
```

### Migration Scope

All occurrences of `plainto_tsquery` in the codebase:

| File | Function | Change |
|------|----------|--------|
| `crates/matric-db/src/search.rs` | `search()` | Replace |
| `crates/matric-db/src/search.rs` | `search_with_strict_filter()` | Replace |
| `crates/matric-db/src/search.rs` | `search_filtered()` | Replace |
| `crates/matric-db/src/search.rs` | `search_by_keyword()` | Replace |
| `crates/matric-db/src/skos_tags.rs` | Tag search | Replace |
| `crates/matric-db/src/embedding_sets.rs` | Set search | Replace |

## Consequences

### Positive

- **OR support**: Users can search "cat OR dog" (fixes #308)
- **NOT support**: Users can exclude terms with "-" prefix
- **Phrase search**: Users can search for exact phrases with quotes
- **Google-like syntax**: Familiar search experience
- **Very high error tolerance**: Never throws parse errors
- **Zero migration required**: Function available since PostgreSQL 11
- **Backward compatible**: Existing plain-text queries work identically

### Negative

- **Behavioral change**: "OR" is now an operator, not a keyword
  - Searching for literal "OR" requires quotes: "\"OR\""
- **Learning curve**: Users need to learn new syntax (optional)
- **Documentation update**: Need to document new query syntax

### Mitigations

1. **Behavioral change**: Document that "OR" is reserved; rare edge case
2. **Learning curve**: Syntax is optional; plain keywords work as before
3. **Documentation**: Add query syntax examples to API docs and MCP tool descriptions

## Alternatives Considered

### 1. to_tsquery() (Full Power)

PostgreSQL's most powerful query parser with explicit operators.

```sql
SELECT to_tsquery('english', 'cat & dog | rat');
```

**Rejected because:**
- Strict syntax: Errors on invalid input
- Not suitable for user-facing search (parse errors would bubble up)
- Requires explicit operators (`&`, `|`, `!`) not intuitive for users

### 2. Custom Query Parser (Application-Level)

Build custom parser that transforms user input to `to_tsquery()` format.

**Rejected because:**
- Significant implementation effort
- Maintenance burden (edge cases, escaping, security)
- `websearch_to_tsquery()` already provides exactly this functionality
- PostgreSQL-maintained solution is more robust

### 3. phraseto_tsquery() (Phrase Only)

Specialized function for phrase search.

**Rejected because:**
- Phrase-only: No OR/NOT support
- `websearch_to_tsquery()` includes phrase support plus more
- Would need to maintain two parsers

### 4. Keep plainto_tsquery() (Status Quo)

Continue with current implementation.

**Rejected because:**
- Fails to address issue #308 (OR operators)
- No phrase search capability
- Users cannot express exclusions

## Implementation

**Code Location:**
- `crates/matric-db/src/search.rs` - Main search functions
- `crates/matric-db/src/skos_tags.rs` - Tag search
- `crates/matric-db/src/embedding_sets.rs` - Embedding set search

**Key Changes:**

```rust
// Before (in all search queries):
plainto_tsquery('matric_english', $1)

// After:
websearch_to_tsquery('matric_english', $1)
```

**Testing:**
- Unit tests for query syntax variations (OR, NOT, phrase, combined)
- Integration tests for search result correctness
- Regression tests for existing plain-text queries

## References

- PostgreSQL websearch_to_tsquery: https://www.postgresql.org/docs/16/textsearch-controls.html#TEXTSEARCH-PARSING-QUERIES
- Issue #308: OR operator not supported in search queries
- Technical Research: `.aiwg/working/discovery/multilingual-fts/spikes/technical-research.md`
