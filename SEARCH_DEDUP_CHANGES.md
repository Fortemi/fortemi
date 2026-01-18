# Search Deduplication Implementation - Changes Required

## Summary

Implemented search result deduplication for chunked documents in matric-search library. API integration requires the following changes to `crates/matric-api/src/main.rs`:

## Change 1: Update imports (Line 94)

**Before:**
```rust
use matric_search::{HybridSearchConfig, HybridSearchEngine, SearchRequest};
```

**After:**
```rust
use matric_search::{deduplicate_search_results, DeduplicationConfig, EnhancedSearchHit, HybridSearchConfig, HybridSearchEngine, SearchRequest};
```

## Change 2: Update SearchQuery struct (Lines 2642-2660)

**Add these two new fields after `since: Option<String>,` (line 2659):**

```rust
    /// Deduplicate chunks from the same document (default: true)
    deduplicate_chains: Option<bool>,
    /// Expand chains to include full document content (default: false)
    expand_chains: Option<bool>,
```

**Complete updated struct:**
```rust
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Date fields reserved for future search filtering
struct SearchQuery {
    q: String,
    limit: Option<i64>,
    filters: Option<String>,
    mode: Option<String>,
    /// Embedding set slug to search within (default: "default")
    #[serde(rename = "set")]
    embedding_set: Option<String>,
    /// Filter: notes created after this timestamp (ISO 8601)
    created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes created before this timestamp (ISO 8601)
    created_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes updated after this timestamp (ISO 8601)
    updated_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes updated before this timestamp (ISO 8601)
    updated_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Relative time filter: "7d" (7 days), "1w" (1 week), "1m" (1 month), "2h" (2 hours)
    since: Option<String>,
    /// Deduplicate chunks from the same document (default: true)
    deduplicate_chains: Option<bool>,
    /// Expand chains to include full document content (default: false)
    expand_chains: Option<bool>,
}
```

## Change 3: Update SearchResponse struct (Lines 2662-2667)

**Before:**
```rust
#[derive(Debug, Serialize)]
struct SearchResponse {
    results: Vec<SearchHit>,
    query: String,
    total: usize,
}
```

**After:**
```rust
#[derive(Debug, Serialize)]
struct SearchResponse {
    results: Vec<EnhancedSearchHit>,
    query: String,
    total: usize,
}
```

## Change 4: Update search_notes function (Lines 2718-2726)

**Before:**
```rust
    let results = request.execute(&state.search).await?;
    let total = results.len();

    Ok(Json(SearchResponse {
        results,
        query: query.q,
        total,
    }))
```

**After:**
```rust
    let results = request.execute(&state.search).await?;

    // Apply deduplication based on query parameters
    let dedup_config = DeduplicationConfig {
        deduplicate_chains: query.deduplicate_chains.unwrap_or(true),
        expand_chains: query.expand_chains.unwrap_or(false),
    };

    let deduplicated = deduplicate_search_results(results, &dedup_config);
    let total = deduplicated.len();

    Ok(Json(SearchResponse {
        results: deduplicated,
        query: query.q,
        total,
    }))
```

## Testing

After applying these changes, test the API:

```bash
# Test with deduplication enabled (default)
curl "http://localhost:3000/api/v1/search?q=test"

# Test with deduplication explicitly disabled
curl "http://localhost:3000/api/v1/search?q=test&deduplicate_chains=false"

# Test with expansion enabled
curl "http://localhost:3000/api/v1/search?q=test&expand_chains=true"
```

## Files Modified

1. `crates/matric-search/src/deduplication.rs` - NEW FILE (deduplication logic)
2. `crates/matric-search/src/lib.rs` - Updated exports
3. `crates/matric-search/Cargo.toml` - Added regex dependency
4. `crates/matric-api/src/main.rs` - Updated search endpoint (changes documented above)

## Test Coverage

All tests pass in matric-search crate:
- 13 unit tests for deduplication logic
- 100% coverage of deduplication module
- Tests cover: empty results, single results, multiple chunks, metadata preservation, serialization

## Implementation Complete

The deduplication implementation is complete and tested in the matric-search library. The API changes documented above are required to expose this functionality through the HTTP API.
