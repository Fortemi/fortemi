//! # matric-db
//!
//! PostgreSQL database layer for matric-memory.
//!
//! This crate provides:
//! - Connection pool management
//! - Repository implementations for all core entities
//! - Full-text search with PostgreSQL tsvector
//! - Vector search with pgvector
//! - W3C SKOS-compliant hierarchical tag system
//! - ColBERT late interaction re-ranking
//!
//! ## Example
//!
//! ```rust,ignore
//! use matric_db::{Database, NoteRepository, CreateNoteRequest};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let db = Database::connect("postgres://localhost/matric").await?;
//!
//!     let note_id = db.notes.insert(CreateNoteRequest {
//!         content: "Hello, world!".to_string(),
//!         format: "markdown".to_string(),
//!         source: "user".to_string(),
//!         collection_id: None,
//!         tags: Some(vec!["greeting".to_string()]),
//!     }).await?;
//!
//!     println!("Created note: {}", note_id);
//!     Ok(())
//! }
//! ```
pub mod archives;
pub mod chunking;
pub mod colbert;
pub mod collections;
pub mod document_types;
pub mod embedding_sets;
pub mod embeddings;
pub mod file_storage;
pub mod hashtag_extraction;
pub mod jobs;
pub mod links;
pub mod memory_search;
pub mod notes;
pub mod oauth;
pub mod pke_keys;
pub mod pke_keysets;
pub mod pool;
pub mod provenance;
pub mod schema_context;
pub mod schema_validation;
pub mod search;
pub mod skos_tags;
mod skos_tags_tx;
pub mod strict_filter;
#[cfg(feature = "tree-sitter")]
pub mod syntactic_chunker;
pub mod tags;
pub mod templates;
pub mod unified_filter;
pub mod versioning;
pub mod webhooks;

#[cfg(test)]
mod tests;

// Test fixtures for integration tests
// Note: Always compiled so integration tests (in tests/) can use DEFAULT_TEST_DATABASE_URL
pub mod test_fixtures;

// Re-export core types
pub use matric_core::*;

/// Escape LIKE/ILIKE wildcard characters (`%`, `_`, `\`) in user input.
pub fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// Re-export chunking types
pub use chunking::{
    Chunk, Chunker, ChunkerConfig, ParagraphChunker, RecursiveChunker, SemanticChunker,
    SentenceChunker, SlidingWindowChunker,
};

#[cfg(feature = "tree-sitter")]
pub use syntactic_chunker::{CodeChunk, CodeUnitKind, SyntacticChunker};

// Re-export hashtag extraction
pub use hashtag_extraction::extract_inline_hashtags;

// Re-export repository implementations
pub use archives::PgArchiveRepository;
pub use colbert::{ColBERTRepository, ColBERTStats, TokenEmbedding};
pub use collections::PgCollectionRepository;
pub use document_types::PgDocumentTypeRepository;
pub use embedding_sets::PgEmbeddingSetRepository;
pub use embeddings::{utils as embedding_utils, PgEmbeddingRepository};
pub use file_storage::{
    compute_content_hash, generate_storage_path, FilesystemBackend, PgFileStorageRepository,
    StorageBackend,
};
pub use jobs::{get_extraction_stats, PgJobRepository};
pub use links::{GraphEdge, GraphNode, GraphResult, PgLinkRepository, TopologyStats};
pub use memory_search::{MemorySearchRepository, PgMemorySearchRepository};
pub use notes::{ListNotesWithFilterRequest, ListNotesWithFilterResponse, PgNoteRepository};
pub use oauth::PgOAuthRepository;
pub use pke_keys::{PgPkeKeyRepository, PkePublicKey};
pub use pke_keysets::{
    CreateKeysetRequest, ExportedKeyset, PgPkeKeysetRepository, PkeKeyset, PkeKeysetSummary,
};
pub use pool::{create_pool, create_pool_with_config, log_pool_metrics, PoolConfig};
pub use provenance::PgProvenanceRepository;
pub use schema_context::SchemaContext;
pub use schema_validation::validate_schema_name;
pub use search::PgFtsSearch;
pub use strict_filter::{QueryParam, StrictFilterQueryBuilder};
pub use tags::PgTagRepository;
pub use templates::PgTemplateRepository;
pub use unified_filter::{UnifiedFilterQueryBuilder, UnifiedFilterResult};
pub use versioning::{
    NoteVersions, OriginalVersion, RevisionVersionSummary, VersionSummary, VersioningRepository,
};
pub use webhooks::PgWebhookRepository;

// Re-export SKOS repository and traits
pub use skos_tags::{
    PgSkosRepository, SkosCollectionRepository, SkosConceptRepository, SkosConceptSchemeRepository,
    SkosGovernanceRepository, SkosLabelRepository, SkosNoteRepository, SkosRelationRepository,
    SkosTagResolutionRepository, SkosTaggingRepository,
};

/// Combined database context with all repositories.
pub struct Database {
    /// The underlying connection pool.
    pub pool: sqlx::Pool<sqlx::Postgres>,
    /// Note repository for CRUD operations.
    pub notes: PgNoteRepository,
    /// Embedding repository for vector storage.
    pub embeddings: PgEmbeddingRepository,
    /// Embedding set repository for managing embedding collections.
    pub embedding_sets: PgEmbeddingSetRepository,
    /// Link repository for note relationships.
    pub links: PgLinkRepository,
    /// Tag repository for simple tag management (legacy).
    pub tags: PgTagRepository,
    /// SKOS repository for W3C-compliant hierarchical tags.
    pub skos: PgSkosRepository,
    /// Collection repository for folder hierarchy.
    pub collections: PgCollectionRepository,
    /// Document type repository for managing document types.
    pub document_types: PgDocumentTypeRepository,
    /// Job repository for background processing.
    pub jobs: PgJobRepository,
    /// Full-text search provider.
    pub search: PgFtsSearch,
    /// W3C PROV provenance tracking repository.
    pub provenance: PgProvenanceRepository,
    /// OAuth2 and API key repository.
    pub oauth: PgOAuthRepository,
    /// Note template repository.
    pub templates: PgTemplateRepository,
    /// Archive schema repository for parallel memory archives.
    pub archives: PgArchiveRepository,
    /// Note version history repository.
    pub versioning: VersioningRepository,
    /// Memory search repository for temporal-spatial queries.
    pub memory_search: PgMemorySearchRepository,
    /// ColBERT token embeddings repository for late interaction re-ranking.
    pub colbert: ColBERTRepository,
    /// File storage repository (note: requires backend configuration).
    /// Use `with_file_storage` to configure.
    pub file_storage: Option<PgFileStorageRepository>,
    /// File storage base path for cloning (used by Clone impl to reconstruct backend).
    file_storage_path: Option<String>,
    /// SKOS tags repository (convenience alias).
    pub skos_tags: PgSkosRepository,
    /// Webhook repository for outbound HTTP notifications (Issue #44).
    pub webhooks: PgWebhookRepository,
    /// PKE public key registry (Issue #113).
    pub pke_keys: PgPkeKeyRepository,
    /// PKE keyset repository for REST API (Issues #328, #332).
    pub pke_keysets: PgPkeKeysetRepository,
}

impl Database {
    /// Create a new Database instance from a connection pool.
    pub fn new(pool: sqlx::Pool<sqlx::Postgres>) -> Self {
        let skos = PgSkosRepository::new(pool.clone());
        Self {
            notes: PgNoteRepository::new(pool.clone()),
            embeddings: PgEmbeddingRepository::new(pool.clone()),
            embedding_sets: PgEmbeddingSetRepository::new(pool.clone()),
            links: PgLinkRepository::new(pool.clone()),
            tags: PgTagRepository::new(pool.clone()),
            skos: skos.clone(),
            collections: PgCollectionRepository::new(pool.clone()),
            document_types: PgDocumentTypeRepository::new(pool.clone()),
            jobs: PgJobRepository::new(pool.clone()),
            search: PgFtsSearch::new(pool.clone()),
            provenance: PgProvenanceRepository::new(pool.clone()),
            oauth: PgOAuthRepository::new(pool.clone()),
            templates: PgTemplateRepository::new(pool.clone()),
            archives: PgArchiveRepository::new(pool.clone()),
            versioning: VersioningRepository::new(pool.clone()),
            memory_search: PgMemorySearchRepository::new(pool.clone()),
            colbert: ColBERTRepository::new(pool.clone()),
            file_storage: None,
            file_storage_path: None,
            skos_tags: skos,
            webhooks: PgWebhookRepository::new(pool.clone()),
            pke_keys: PgPkeKeyRepository::new(pool.clone()),
            pke_keysets: PgPkeKeysetRepository::new(pool.clone()),
            pool,
        }
    }

    /// Configure file storage with a backend and inline threshold.
    ///
    /// # Arguments
    ///
    /// * `backend` - Storage backend (filesystem, S3, etc.)
    /// * `inline_threshold` - Files smaller than this (in bytes) are stored inline
    pub fn with_file_storage(
        mut self,
        backend: impl StorageBackend + 'static,
        inline_threshold: i64,
    ) -> Self {
        self.file_storage = Some(PgFileStorageRepository::new(
            self.pool.clone(),
            backend,
            inline_threshold,
        ));
        self
    }

    /// Configure file storage with a filesystem backend path.
    ///
    /// Unlike `with_file_storage`, this stores the path so that `Clone` can
    /// reconstruct the backend correctly (instead of using a `/dev/null` placeholder).
    pub fn with_filesystem_storage(mut self, path: &str, inline_threshold: i64) -> Self {
        self.file_storage = Some(PgFileStorageRepository::new(
            self.pool.clone(),
            FilesystemBackend::new(path),
            inline_threshold,
        ));
        self.file_storage_path = Some(path.to_string());
        self
    }

    /// Create a new Database instance by connecting to the given URL.
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = create_pool(url).await?;
        Ok(Self::new(pool))
    }

    /// Create with custom pool configuration.
    pub async fn connect_with_config(url: &str, config: PoolConfig) -> Result<Self> {
        let pool = create_pool_with_config(url, config).await?;
        Ok(Self::new(pool))
    }

    /// Connect to test database (for integration tests).
    #[cfg(test)]
    pub async fn connect_test() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| crate::test_fixtures::DEFAULT_TEST_DATABASE_URL.to_string());
        Self::connect(&database_url).await
    }

    /// Run pending migrations.
    #[cfg(feature = "migrations")]
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .map_err(|e| Error::Database(sqlx::Error::Migrate(Box::new(e))))?;
        Ok(())
    }

    /// Get the underlying connection pool.
    pub fn pool(&self) -> &sqlx::Pool<sqlx::Postgres> {
        &self.pool
    }

    /// Create a schema-scoped database context for the specified schema.
    ///
    /// All operations executed through the returned context will have their
    /// search_path set to the specified schema, providing data isolation for
    /// parallel memory archives.
    ///
    /// # Arguments
    ///
    /// * `schema` - The schema name to scope operations to
    ///
    /// # Returns
    ///
    /// A `SchemaContext` instance scoped to the specified schema
    ///
    /// # Errors
    ///
    /// Returns an error if the schema name is invalid or unsafe.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let ctx = db.for_schema("archive_2026")?;
    /// ctx.execute(|tx| async move {
    ///     // Operations here are scoped to archive_2026 schema
    ///     sqlx::query("SELECT * FROM note").fetch_all(&mut **tx).await
    /// }).await?;
    /// ```
    pub fn for_schema(&self, schema: &str) -> Result<SchemaContext> {
        SchemaContext::new(self.pool.clone(), schema)
    }

    /// Create a schema-scoped database context for the default (public) schema.
    ///
    /// This is equivalent to `for_schema("public")` but provides a convenient
    /// method for accessing the default schema.
    ///
    /// # Returns
    ///
    /// A `SchemaContext` instance scoped to the public schema
    pub fn default_schema(&self) -> SchemaContext {
        // "public" is always valid, so unwrap is safe
        SchemaContext::new(self.pool.clone(), "public")
            .expect("public schema should always be valid")
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        let skos = PgSkosRepository::new(self.pool.clone());
        Self {
            pool: self.pool.clone(),
            notes: PgNoteRepository::new(self.pool.clone()),
            embeddings: PgEmbeddingRepository::new(self.pool.clone()),
            embedding_sets: PgEmbeddingSetRepository::new(self.pool.clone()),
            links: PgLinkRepository::new(self.pool.clone()),
            tags: PgTagRepository::new(self.pool.clone()),
            skos: skos.clone(),
            collections: PgCollectionRepository::new(self.pool.clone()),
            document_types: PgDocumentTypeRepository::new(self.pool.clone()),
            jobs: PgJobRepository::new(self.pool.clone()),
            search: PgFtsSearch::new(self.pool.clone()),
            provenance: PgProvenanceRepository::new(self.pool.clone()),
            oauth: PgOAuthRepository::new(self.pool.clone()),
            templates: PgTemplateRepository::new(self.pool.clone()),
            archives: PgArchiveRepository::new(self.pool.clone()),
            versioning: VersioningRepository::new(self.pool.clone()),
            memory_search: PgMemorySearchRepository::new(self.pool.clone()),
            colbert: ColBERTRepository::new(self.pool.clone()),
            file_storage: self.file_storage_path.as_ref().map(|path| {
                PgFileStorageRepository::new(
                    self.pool.clone(),
                    FilesystemBackend::new(path),
                    10_485_760,
                )
            }),
            file_storage_path: self.file_storage_path.clone(),
            skos_tags: skos,
            webhooks: PgWebhookRepository::new(self.pool.clone()),
            pke_keys: PgPkeKeyRepository::new(self.pool.clone()),
            pke_keysets: PgPkeKeysetRepository::new(self.pool.clone()),
        }
    }
}
