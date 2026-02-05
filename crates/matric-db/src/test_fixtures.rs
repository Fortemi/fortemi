//! Test fixtures for database integration tests.
//!
//! Provides reusable setup/teardown functions and test data builders for
//! consistent testing across the codebase.
//!
//! ## Configuration
//!
//! The test database URL is configured via the `DATABASE_URL` environment variable.
//! If not set, defaults to [`DEFAULT_TEST_DATABASE_URL`].
//!
//! ## Usage
//!
//! ```rust,ignore
//! use matric_db::test_fixtures::{TestDatabase, TestDataBuilder};
//!
//! #[tokio::test]
//! async fn test_something() {
//!     let test_db = TestDatabase::new().await;
//!     let data = TestDataBuilder::new(&test_db.db)
//!         .with_note("Test content")
//!         .with_tag("tutorial")
//!         .build()
//!         .await;
//!
//!     // Run your tests...
//!
//!     test_db.cleanup().await;
//! }
//! ```

/// Default test database URL when DATABASE_URL is not set.
///
/// Uses port 15432 to avoid conflicts with production databases.
/// See `docs/quick-start-testing.md` for test database setup instructions.
pub const DEFAULT_TEST_DATABASE_URL: &str = "postgres://matric:matric@localhost:15432/matric_test";

use crate::{
    colbert::ColBERTRepository,
    collections::PgCollectionRepository,
    document_types::PgDocumentTypeRepository,
    embedding_sets::PgEmbeddingSetRepository,
    embeddings::PgEmbeddingRepository,
    links::PgLinkRepository,
    notes::PgNoteRepository,
    oauth::PgOAuthRepository,
    pool::create_pool_with_config,
    search::PgFtsSearch,
    skos_tags::{PgSkosRepository, SkosConceptRepository, SkosConceptSchemeRepository},
    tags::PgTagRepository,
    templates::PgTemplateRepository,
    CollectionRepository, CreateNoteRequest, NoteRepository, PoolConfig,
};
use matric_core::{CreateConceptRequest, CreateConceptSchemeRequest};
use pgvector::Vector;
use sqlx::PgPool;
use uuid::Uuid;

/// Test database connection with automatic cleanup.
pub struct TestDatabase {
    pub pool: PgPool,
    pub db: TestDb,
    schema_name: String,
    cleanup_on_drop: bool,
}

impl TestDatabase {
    /// Create a new test database instance.
    ///
    /// By default, connects to `DATABASE_URL` environment variable or
    /// `postgres://matric:matric@localhost:15432/matric_test`.
    pub async fn new() -> Self {
        Self::with_cleanup(true).await
    }

    /// Create a test database without automatic cleanup (useful for debugging).
    pub async fn without_cleanup() -> Self {
        Self::with_cleanup(false).await
    }

    async fn with_cleanup(cleanup: bool) -> Self {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_TEST_DATABASE_URL.to_string());

        let config = PoolConfig {
            max_connections: 5,
            min_connections: 1,
            connect_timeout: std::time::Duration::from_secs(30),
            idle_timeout: std::time::Duration::from_secs(600),
            max_lifetime: Some(std::time::Duration::from_secs(1800)),
        };

        let pool = create_pool_with_config(&database_url, config)
            .await
            .expect("Failed to create test database pool");

        // Create unique schema for test isolation
        let schema_name = format!("test_{}", Uuid::new_v4().to_string().replace('-', "_"));

        sqlx::query(&format!("CREATE SCHEMA {}", schema_name))
            .execute(&pool)
            .await
            .expect("Failed to create test schema");

        // Set search path for this connection
        sqlx::query(&format!("SET search_path TO {}, public", schema_name))
            .execute(&pool)
            .await
            .expect("Failed to set search path");

        let db = TestDb {
            pool: pool.clone(),
            notes: PgNoteRepository::new(pool.clone()),
            tags: PgTagRepository::new(pool.clone()),
            skos_tags: PgSkosRepository::new(pool.clone()),
            collections: PgCollectionRepository::new(pool.clone()),
            templates: PgTemplateRepository::new(pool.clone()),
            embeddings: PgEmbeddingRepository::new(pool.clone()),
            embedding_sets: PgEmbeddingSetRepository::new(pool.clone()),
            links: PgLinkRepository::new(pool.clone()),
            search: PgFtsSearch::new(pool.clone()),
            colbert: ColBERTRepository::new(pool.clone()),
            document_types: PgDocumentTypeRepository::new(pool.clone()),
            oauth: PgOAuthRepository::new(pool.clone()),
        };

        Self {
            pool: pool.clone(),
            db,
            schema_name,
            cleanup_on_drop: cleanup,
        }
    }

    /// Manually clean up test data and drop schema.
    pub async fn cleanup(mut self) {
        if self.cleanup_on_drop {
            self.cleanup_impl().await;
            self.cleanup_on_drop = false; // Prevent double cleanup
        }
    }

    async fn cleanup_impl(&self) {
        // Drop the test schema and all its contents
        let _ = sqlx::query(&format!(
            "DROP SCHEMA IF EXISTS {} CASCADE",
            self.schema_name
        ))
        .execute(&self.pool)
        .await;
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            // Spawn blocking task for async cleanup in Drop
            let pool = self.pool.clone();
            let schema = self.schema_name.clone();
            tokio::spawn(async move {
                let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS {} CASCADE", schema))
                    .execute(&pool)
                    .await;
            });
        }
    }
}

/// Repository collection for tests.
pub struct TestDb {
    pub pool: PgPool,
    pub notes: PgNoteRepository,
    pub tags: PgTagRepository,
    pub skos_tags: PgSkosRepository,
    pub collections: PgCollectionRepository,
    pub templates: PgTemplateRepository,
    pub embeddings: PgEmbeddingRepository,
    pub embedding_sets: PgEmbeddingSetRepository,
    pub links: PgLinkRepository,
    pub search: PgFtsSearch,
    pub colbert: ColBERTRepository,
    pub document_types: PgDocumentTypeRepository,
    pub oauth: PgOAuthRepository,
}

/// Builder for test data with fluent API.
pub struct TestDataBuilder<'a> {
    db: &'a TestDb,
    created_notes: Vec<Uuid>,
    created_tags: Vec<Uuid>,
    created_concepts: Vec<Uuid>,
    created_collections: Vec<Uuid>,
}

impl<'a> TestDataBuilder<'a> {
    pub fn new(db: &'a TestDb) -> Self {
        Self {
            db,
            created_notes: Vec::new(),
            created_tags: Vec::new(),
            created_concepts: Vec::new(),
            created_collections: Vec::new(),
        }
    }

    /// Create a test note with given content.
    pub async fn with_note(mut self, content: &str) -> Self {
        let note_id = self
            .db
            .notes
            .insert(CreateNoteRequest {
                content: content.to_string(),
                format: "markdown".to_string(),
                source: "test".to_string(),
                collection_id: None,
                tags: None,
                metadata: None,
                document_type_id: None,
            })
            .await
            .expect("Failed to create test note");

        self.created_notes.push(note_id);
        self
    }

    /// Create a test note with tags.
    pub async fn with_tagged_note(mut self, content: &str, tags: &[&str]) -> Self {
        let note_id = self
            .db
            .notes
            .insert(CreateNoteRequest {
                content: content.to_string(),
                format: "markdown".to_string(),
                source: "test".to_string(),
                collection_id: None,
                tags: Some(tags.iter().map(|s| s.to_string()).collect()),
                metadata: None,
                document_type_id: None,
            })
            .await
            .expect("Failed to create test note");

        self.created_notes.push(note_id);
        self
    }

    /// Create multiple notes with similar content for search testing.
    pub async fn with_search_corpus(mut self, count: usize) -> Self {
        let corpus = vec![
            "Quantum computing uses qubits for computation",
            "Machine learning is a subset of artificial intelligence",
            "Neural networks are inspired by biological neurons",
            "Deep learning uses multi-layer neural networks",
            "Natural language processing analyzes human language",
            "Computer vision enables machines to understand images",
            "Reinforcement learning trains agents through rewards",
            "Supervised learning uses labeled training data",
            "Unsupervised learning discovers patterns in data",
            "Transfer learning reuses pre-trained models",
        ];

        for i in 0..count {
            let content = corpus[i % corpus.len()];
            self = self.with_note(content).await;
        }

        self
    }

    /// Create a SKOS concept (tag).
    pub async fn with_concept(mut self, pref_label: &str, scheme_id: Option<Uuid>) -> Self {
        let scheme = if let Some(id) = scheme_id {
            id
        } else {
            // Create default scheme if none provided
            let default_scheme_id = self
                .db
                .skos_tags
                .create_scheme(CreateConceptSchemeRequest {
                    notation: "test".to_string(),
                    title: "Test Scheme".to_string(),
                    uri: None,
                    description: Some("Default test scheme".to_string()),
                    creator: None,
                    publisher: None,
                    rights: None,
                    version: None,
                })
                .await
                .expect("Failed to create default scheme");
            default_scheme_id
        };

        let concept_id = self
            .db
            .skos_tags
            .create_concept(CreateConceptRequest {
                scheme_id: scheme,
                notation: None,
                pref_label: pref_label.to_string(),
                language: "en".to_string(),
                status: Default::default(),
                facet_type: None,
                facet_source: None,
                facet_domain: None,
                facet_scope: None,
                definition: None,
                scope_note: None,
                broader_ids: vec![],
                related_ids: vec![],
                alt_labels: vec![],
            })
            .await
            .expect("Failed to create concept");

        self.created_concepts.push(concept_id);
        self
    }

    /// Create a collection.
    pub async fn with_collection(mut self, name: &str, parent_id: Option<Uuid>) -> Self {
        let collection_id = self
            .db
            .collections
            .create(name, None, parent_id)
            .await
            .expect("Failed to create collection");

        self.created_collections.push(collection_id);
        self
    }

    /// Build and return the test data.
    pub async fn build(self) -> TestData {
        TestData {
            notes: self.created_notes,
            tags: self.created_tags,
            concepts: self.created_concepts,
            collections: self.created_collections,
        }
    }
}

/// Test data created by the builder.
#[derive(Debug)]
pub struct TestData {
    pub notes: Vec<Uuid>,
    pub tags: Vec<Uuid>,
    pub concepts: Vec<Uuid>,
    pub collections: Vec<Uuid>,
}

/// Seed minimal test data for basic operations.
pub async fn seed_minimal_data(db: &TestDb) -> TestData {
    TestDataBuilder::new(db)
        .with_note("Test note 1")
        .await
        .with_note("Test note 2")
        .await
        .with_concept("TestConcept", None)
        .await
        .with_collection("TestCollection", None)
        .await
        .build()
        .await
}

/// Seed a corpus for search testing (100+ notes).
pub async fn seed_search_corpus(db: &TestDb) -> TestData {
    TestDataBuilder::new(db)
        .with_search_corpus(100)
        .await
        .build()
        .await
}

/// Seed data for embedding tests.
pub async fn seed_embedding_corpus(db: &TestDb, dimension: usize) -> Result<TestData, sqlx::Error> {
    let mut builder = TestDataBuilder::new(db);

    // Create notes with embeddings
    for i in 0..50 {
        builder = builder
            .with_note(&format!("Embedding test note {}", i))
            .await;
    }

    let data = builder.build().await;

    // Add embeddings to some notes
    for (idx, note_id) in data.notes.iter().enumerate().take(25) {
        let embedding = vec![0.1 * idx as f32; dimension];
        let vector = Vector::from(embedding);

        sqlx::query(
            "INSERT INTO embedding (note_id, vector, model, created_at_utc)
             VALUES ($1, $2, 'test-model', NOW())",
        )
        .bind(note_id)
        .bind(vector)
        .execute(&db.pool)
        .await?;
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL with migrated database
    async fn test_database_creation() {
        let test_db = TestDatabase::new().await;
        assert!(test_db.pool.size() > 0);
        test_db.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL with migrated database
    async fn test_data_builder_notes() {
        let test_db = TestDatabase::new().await;
        let data = TestDataBuilder::new(&test_db.db)
            .with_note("Test 1")
            .await
            .with_note("Test 2")
            .await
            .build()
            .await;

        assert_eq!(data.notes.len(), 2);
        test_db.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL with migrated database
    async fn test_seed_minimal_data() {
        let test_db = TestDatabase::new().await;
        let data = seed_minimal_data(&test_db.db).await;

        assert!(data.notes.len() >= 2);
        assert!(!data.concepts.is_empty());
        assert!(!data.collections.is_empty());

        test_db.cleanup().await;
    }

    #[tokio::test]
    #[ignore] // Requires DATABASE_URL with migrated database
    async fn test_seed_search_corpus() {
        let test_db = TestDatabase::new().await;
        let data = seed_search_corpus(&test_db.db).await;

        assert_eq!(data.notes.len(), 100);
        test_db.cleanup().await;
    }
}
