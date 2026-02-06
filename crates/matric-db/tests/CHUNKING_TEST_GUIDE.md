# Document Chunking Integration Test Guide

## Overview

The `chunking_integration_test.rs` file provides comprehensive integration tests for the document chunking pipeline in matric-memory. These tests verify the complete workflow from chunk creation to search deduplication.

## Test Coverage

### Test Categories

#### 1. Chain Creation Tests
- **test_large_document_creates_chunk_chain**: Verifies creation of multi-chunk documents with proper metadata linking
  - Tests: Chain ID consistency, sequence numbering, total chunk count
- **test_chunk_metadata_structure**: Validates the JSONB structure of chunk_metadata
  - Tests: Required fields (chain_id, chunk_sequence, total_chunks, chunking_strategy)
- **test_single_chunk_document**: Edge case for documents with only one chunk
  - Tests: Metadata correctness for minimal chunking

#### 2. Reconstruction Tests
- **test_full_document_reconstruction**: Verifies reassembly of chunked documents
  - Tests: Content preservation, ordering, completeness
- **test_reconstruction_missing_chunk**: Handles incomplete document chains
  - Tests: Detection of gaps in sequence numbers

#### 3. Search Deduplication Tests
- **test_search_query_matches_multiple_chunks**: Ensures FTS finds all matching chunks
  - Tests: Query matching across chunks, chain_id filtering
- **test_deduplication_keeps_highest_score**: Validates ranking logic
  - Tests: Best-scoring chunk identification, score ordering

#### 4. Edge Cases
- **test_document_at_exact_threshold**: Boundary condition testing
- **test_empty_chunk_content**: Handles empty content gracefully
- **test_unicode_content_in_chunks**: UTF-8 preservation across languages
- **test_large_number_of_chunks**: Stress test with 50 chunks
- **test_chunk_with_special_characters**: SQL/JSON escaping validation
- **test_chunk_metadata_indexing**: Verifies GIN index usage on JSONB

## Coverage Metrics

| Category | Tests | Coverage Target | Status |
|----------|-------|-----------------|--------|
| Chain Creation | 3 | 100% | Complete |
| Reconstruction | 2 | 100% | Complete |
| Search Deduplication | 2 | 100% | Complete |
| Edge Cases | 6 | 100% | Complete |
| **Total** | **13** | **100%** | **Complete** |

## Running the Tests

### Prerequisites

1. **PostgreSQL Database**: Tests require a fully migrated database
   ```bash
   # Set database URL (or use default)
   export DATABASE_URL="postgres://matric:matric@localhost/matric"

   # Run migrations
   sqlx migrate run
   ```

2. **Database Schema**: Ensure migration `20260122000000_add_chunk_metadata.sql` has been applied

### Running All Tests

```bash
# Run all chunking integration tests
cargo test --package matric-db --test chunking_integration_test

# Run specific test
cargo test --package matric-db --test chunking_integration_test test_large_document_creates_chunk_chain
```

### Test Execution Notes

- Tests require DATABASE_URL pointing to a migrated PostgreSQL database
- Tests clean up after themselves (delete test data)
- Tests use real database transactions (not mocked)
- Each test is isolated and can run independently

## Database Schema

Tests rely on the following schema (from migration 20260122000000):

```sql
-- chunk_metadata JSONB column structure
{
  "chain_id": "uuid",           -- Links chunks together
  "chunk_sequence": 0,          -- Position in document (0-based)
  "total_chunks": 3,            -- Total number of chunks
  "chunking_strategy": "semantic" -- Strategy used for chunking
}
```

### Indexes Used
- `idx_note_chunk_metadata`: GIN index for JSONB queries
- `idx_note_chunked`: Index for chunked notes filtering

## Test Data Patterns

### Test Fixture Factory: `TestContext::create_chunked_note`

```rust
async fn create_chunked_note(
    &self,
    chain_id: Uuid,      // Shared ID for document chain
    sequence: u32,       // 0-based position
    total: u32,          // Total chunks in chain
    content: &str        // Chunk text content
) -> Result<Uuid, sqlx::Error>
```

### Example Usage

```rust
let ctx = TestContext::new().await;
let chain_id = Uuid::new_v4();

// Create 3-chunk document
let chunk1 = ctx.create_chunked_note(chain_id, 0, 3, "Part 1").await?;
let chunk2 = ctx.create_chunked_note(chain_id, 1, 3, "Part 2").await?;
let chunk3 = ctx.create_chunked_note(chain_id, 2, 3, "Part 3").await?;

// Query chain
let chunks = ctx.get_chain_chunks(chain_id).await?;
assert_eq!(chunks.len(), 3);

// Cleanup
ctx.cleanup_note(chunk1).await;
ctx.cleanup_note(chunk2).await;
ctx.cleanup_note(chunk3).await;
```

## Assertions and Validation

### Chain Integrity Checks
```rust
// Verify chain_id consistency
for chunk_id in [chunk1, chunk2, chunk3] {
    let metadata = ctx.get_chunk_metadata(chunk_id).await?;
    let stored_chain_id = metadata["chain_id"].as_str()?;
    assert_eq!(stored_chain_id, chain_id.to_string());
}

// Verify sequence order
let sequences: Vec<u32> = chunks.iter().map(|(_, seq)| *seq).collect();
assert_eq!(sequences, vec![0, 1, 2]);
```

### Search Result Validation
```rust
// Query with FTS
let rows = sqlx::query(
    "SELECT n.id, ts_rank(...) as rank
     FROM note n
     WHERE chunk_metadata->>'chain_id' = $1
     ORDER BY rank DESC"
).bind(chain_id.to_string()).fetch_all(&pool).await?;

// Verify deduplication logic
assert_eq!(rows[0].get::<Uuid, _>("id"), highest_scoring_chunk_id);
```

## Error Scenarios Tested

| Scenario | Test | Expected Behavior |
|----------|------|-------------------|
| Missing chunk in sequence | test_reconstruction_missing_chunk | Detect gap, return incomplete chain |
| Empty content | test_empty_chunk_content | Accept gracefully, metadata intact |
| Special characters | test_chunk_with_special_characters | Preserve all characters |
| Unicode text | test_unicode_content_in_chunks | UTF-8 preservation |
| 50+ chunks | test_large_number_of_chunks | Handle large chains |

## Integration with Other Components

### Deduplication Service
The search tests validate that `matric-search/src/deduplication.rs` can:
- Group chunks by chain_id
- Select highest-scoring chunk
- Return ChainSearchInfo metadata

### Reconstruction Service
Tests verify `matric-api/src/services/reconstruction_service.rs` can:
- Query all chunks in a chain
- Sort by sequence number
- Stitch content together
- Extract original title

## Debugging Tips

### View Test Database State
```sql
-- See all chunks for a chain
SELECT id, chunk_metadata->>'chunk_sequence' as seq,
       substring(content from 1 for 50) as preview
FROM note n
JOIN note_original no ON no.note_id = n.id
WHERE chunk_metadata->>'chain_id' = 'your-chain-id-here'
ORDER BY (chunk_metadata->>'chunk_sequence')::int;

-- Check JSONB index usage
EXPLAIN ANALYZE
SELECT * FROM note
WHERE chunk_metadata @> '{"chain_id": "uuid"}'::jsonb;
```

### Common Issues

1. **Migration not applied**: Ensure `chunk_metadata` column exists
   ```bash
   psql -d matric -c "\d note" | grep chunk_metadata
   ```

2. **Index not created**: Verify GIN index exists
   ```sql
   SELECT indexname FROM pg_indexes
   WHERE tablename = 'note' AND indexname LIKE '%chunk%';
   ```

3. **Test data not cleaned**: Check for leftover test notes
   ```sql
   SELECT count(*) FROM note WHERE source = 'test';
   ```

## Performance Benchmarks

Expected test execution times (with database):
- Single test: ~50-200ms
- Full suite (13 tests): ~2-5 seconds
- Large chunk test (50 chunks): ~500ms-1s

## Future Enhancements

Potential additional test scenarios:
- [ ] Concurrent chunk creation race conditions
- [ ] Chunk deletion and orphan detection
- [ ] Chunk update and re-sequencing
- [ ] Cross-collection chunk chains
- [ ] Chunk overlap verification
- [ ] Performance profiling for large documents

## References

- Migration: `migrations/20260122000000_add_chunk_metadata.sql`
- Deduplication: `crates/matric-search/src/deduplication.rs`
- Chunking Service: `crates/matric-api/src/services/chunking_service.rs`
- Reconstruction: `crates/matric-api/src/services/reconstruction_service.rs`
- Issue: #335 (Document Chunking Pipeline Tests)
