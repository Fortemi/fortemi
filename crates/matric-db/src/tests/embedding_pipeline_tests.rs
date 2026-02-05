//! Integration tests for embedding pipeline fixes (#217, #220, #226, #272, #214)
//!
//! These tests require a running PostgreSQL database and should be run with:
//! ```
//! cargo test -p matric-db -- --ignored
//! ```
//!
//! Prerequisites:
//! - PostgreSQL running with matric_test database
//! - Migrations applied
//! - DATABASE_URL environment variable set
//!
//! The embedding pipeline tests validate:
//! - Auto-refresh embedding sets (#220)
//! - Default embedding set document_count (#226)
//! - SKOS concept changes triggering re-embedding (#214)
//! - Semantic search returning results (#217, #272)
//!
//! See the migration `20260205000000_fix_embedding_pipeline.sql` for implementation details.

// Integration tests are in the tests/ directory and require a database connection.
// Run them with: cargo test -p matric-db --test '*' -- --ignored
