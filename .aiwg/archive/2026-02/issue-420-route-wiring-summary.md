# Issue #420: Wire Document Type API Routes - Implementation Summary

## Task
Wire up the document type API handlers in `crates/matric-api/src/main.rs`.

## Changes Made

### 1. Module Declaration (`crates/matric-api/src/main.rs`, lines 5-6)

Added module declaration with explicit path:

```rust
#[path = "handlers/document_types.rs"]
mod document_types;
```

**Location**: After `mod query_types;` (line 4)

### 2. Handler Imports (`crates/matric-api/src/main.rs`, lines 126-129)

Added imports for all document type handler functions:

```rust
use document_types::{
    list_document_types, get_document_type, create_document_type,
    update_document_type, delete_document_type, detect_document_type,
};
```

**Location**: After the `use handlers::{...}` block (after line 124)

### 3. Route Registration (`crates/matric-api/src/main.rs`, lines 574-585)

Added routes in the Axum router:

```rust
// Document Types
.route(
    "/api/v1/document-types",
    get(list_document_types).post(create_document_type),
)
.route("/api/v1/document-types/detect", post(detect_document_type))
.route(
    "/api/v1/document-types/:name",
    get(get_document_type)
        .patch(update_document_type)
        .delete(delete_document_type),
)
```

**Location**: After the Templates section (after line 573), before OAuth2 endpoints

**Critical ordering**: The `/detect` route MUST come before `/:name` to avoid path conflicts.

### 4. Additional Fix (`crates/matric-api/src/handlers/document_types.rs`)

Added missing trait import:

```rust
use matric_core::DocumentTypeRepository;
```

Removed unused imports to fix warnings:
- `response::IntoResponse`
- `Serialize` from serde
- `uuid::Uuid`

## Routes Exposed

The following endpoints are now wired and will be available once dependencies are implemented:

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/api/v1/document-types` | `list_document_types` | List all document types (optionally filtered by category) |
| POST | `/api/v1/document-types` | `create_document_type` | Create a new document type |
| POST | `/api/v1/document-types/detect` | `detect_document_type` | Auto-detect document type from filename/content |
| GET | `/api/v1/document-types/:name` | `get_document_type` | Get document type by name |
| PATCH | `/api/v1/document-types/:name` | `update_document_type` | Update existing document type |
| DELETE | `/api/v1/document-types/:name` | `delete_document_type` | Delete document type (non-system only) |

## Verification

### Tests Created

1. **`crates/matric-api/tests/document_types_routes_test.rs`**
   - Documents expected route structure
   - Verifies route ordering (detect before :name)
   - Tests handler function list

2. **`crates/matric-api/tests/document_types_wiring_test.rs`**
   - Validates module structure
   - Verifies imports are complete
   - Tests route registration order
   - Validates HTTP method assignments
   - Documents expected API behavior

### Current Compilation Status

The route wiring is **syntactically correct** and ready to use. However, full compilation currently fails due to:

1. Incomplete document type models in `matric-core` (missing `DocumentCategory::Agentic` variant)
2. Missing `document_type_id` fields in various request structs
3. Incomplete database repository implementation in `matric-db`

These are **not issues with the route wiring** - they are part of the broader Document Type Registry implementation that needs to be completed.

### Manual Verification Commands

Once dependencies are implemented, verify with:

```bash
# Build the workspace
cargo build --workspace

# Run tests
cargo test --workspace

# Run clippy
cargo clippy -- -D warnings

# Start the API server
cargo run --bin matric-api

# Test endpoints (examples)
curl http://localhost:3000/api/v1/document-types
curl -X POST http://localhost:3000/api/v1/document-types \
  -H "Content-Type: application/json" \
  -d '{"name":"rust","category":"code","extensions":["rs"]}'
curl http://localhost:3000/api/v1/document-types/rust
```

## Files Modified

1. `/home/roctinam/dev/matric-memory/crates/matric-api/src/main.rs`
   - Lines 5-6: Module declaration
   - Lines 126-129: Imports
   - Lines 574-585: Route registration

2. `/home/roctinam/dev/matric-memory/crates/matric-api/src/handlers/document_types.rs`
   - Added trait import
   - Removed unused imports

## Files Created

1. `/home/roctinam/dev/matric-memory/crates/matric-api/tests/document_types_routes_test.rs`
   - Route specification tests

2. `/home/roctinam/dev/matric-memory/crates/matric-api/tests/document_types_wiring_test.rs`
   - Wiring verification tests

## Next Steps

For the Document Type Registry epic to be complete, the following dependencies need to be implemented:

1. **matric-core** (#418):
   - Complete DocumentCategory enum with all variants
   - Add document_type_id fields to request structs
   - Implement all document type models per ADR-025

2. **matric-db** (#419):
   - Complete PgDocumentTypeRepository implementation
   - Add database schema migrations
   - Implement all trait methods

Once these are complete, the wired routes will be fully functional.

## Acceptance Criteria

- [x] Module declaration added with correct path
- [x] All 6 handler functions imported
- [x] Routes registered in correct order (detect before :name)
- [x] All HTTP methods correctly mapped (GET, POST, PATCH, DELETE)
- [x] Tests created to document expected behavior
- [ ] Full compilation passes (blocked by upstream dependencies)
- [ ] Clippy passes with no warnings (blocked by upstream dependencies)
- [ ] Integration tests pass (blocked by upstream dependencies)

## Summary

The route wiring task is **complete**. All routes are properly declared, imported, and registered in the correct order with the appropriate HTTP methods. The implementation is syntactically correct and follows the established patterns in main.rs. The routes will be fully functional once the dependent model and repository implementations are completed.
