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
pub mod call_sessions;
pub mod chunking;
pub mod colbert;
pub mod collections;
pub mod document_types;
pub mod embedding_sets;
pub mod embeddings;
pub mod file_storage;
pub mod hashtag_extraction;
pub mod inbound_sources;
pub mod incoming_webhooks;
pub mod jobs;
pub mod links;
pub mod memory_search;
pub mod notes;
pub mod oauth;
pub mod outbox;
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
pub mod tus;
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
pub use inbound_sources::PgInboundSourceRepository;
pub use incoming_webhooks::{
    validate_incoming_webhook_payload, PgIncomingWebhookReceiverRepository,
};

// Re-export repository implementations
pub use archives::PgArchiveRepository;
pub use call_sessions::PgCallSessionRepository;
pub use colbert::{ColBERTRepository, ColBERTStats, TokenEmbedding};
pub use collections::PgCollectionRepository;
pub use document_types::PgDocumentTypeRepository;
pub use embedding_sets::PgEmbeddingSetRepository;
pub use embeddings::{utils as embedding_utils, PgEmbeddingRepository};
pub use file_storage::{
    compute_content_hash, generate_storage_path, FileDownloadInfo, FileSource, FilesystemBackend,
    PgFileStorageRepository, StagedShardBlob, StagedShardBlobPromotion, StorageBackend,
};
pub use jobs::{get_extraction_stats, PgJobRepository};
pub use links::{
    CoarseCommunityResult, DiagnosticsComparison, DiagnosticsSnapshot, GraphDiagnostics, GraphEdge,
    GraphMeta, GraphNode, GraphResult, PfnetResult, PgLinkRepository, SnnResult, TopologyStats,
};
pub use memory_search::{MemorySearchRepository, PgMemorySearchRepository};
pub use notes::{ListNotesWithFilterRequest, ListNotesWithFilterResponse, PgNoteRepository};
pub use oauth::PgOAuthRepository;
pub use outbox::{CreateOutboxEvent, EventOutboxRecord, PgEventOutboxRepository};
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
pub use tus::PgTusRepository;
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
    /// Shared notify handle for event-driven job worker wake (Issue #417).
    job_notify: std::sync::Arc<tokio::sync::Notify>,
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
    /// Incoming webhook receiver registrations for provider callbacks.
    pub incoming_webhooks: PgIncomingWebhookReceiverRepository,
    /// Inbound external event source connectors + DLQ (#833, Phase D).
    pub inbound_sources: PgInboundSourceRepository,
    /// Shared durable event outbox for write-path event publication.
    pub outbox: PgEventOutboxRepository,
    /// PKE public key registry (Issue #113).
    pub pke_keys: PgPkeKeyRepository,
    /// PKE keyset repository for REST API (Issues #328, #332).
    pub pke_keysets: PgPkeKeysetRepository,
    /// Tus resumable upload session repository (Issue #528).
    pub tus: PgTusRepository,
    /// Provider-agnostic real-time call session repository (Issues #839/#845).
    pub call_sessions: PgCallSessionRepository,
}

impl Database {
    /// Create a new Database instance from a connection pool.
    pub fn new(pool: sqlx::Pool<sqlx::Postgres>) -> Self {
        let skos = PgSkosRepository::new(pool.clone());
        let job_notify = std::sync::Arc::new(tokio::sync::Notify::new());
        Self {
            notes: PgNoteRepository::new(pool.clone()),
            embeddings: PgEmbeddingRepository::new(pool.clone()),
            embedding_sets: PgEmbeddingSetRepository::new(pool.clone()),
            links: PgLinkRepository::new(pool.clone()),
            tags: PgTagRepository::new(pool.clone()),
            skos: skos.clone(),
            collections: PgCollectionRepository::new(pool.clone()),
            document_types: PgDocumentTypeRepository::new(pool.clone()),
            jobs: PgJobRepository::with_notify(pool.clone(), job_notify.clone()),
            job_notify,
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
            incoming_webhooks: PgIncomingWebhookReceiverRepository::new(pool.clone()),
            inbound_sources: PgInboundSourceRepository::new(pool.clone()),
            outbox: PgEventOutboxRepository::new(pool.clone()),
            pke_keys: PgPkeKeyRepository::new(pool.clone()),
            pke_keysets: PgPkeKeysetRepository::new(pool.clone()),
            tus: PgTusRepository::new(pool.clone()),
            call_sessions: PgCallSessionRepository::new(pool.clone()),
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

    /// Return the configured filesystem backend without exposing its base path.
    ///
    /// Knowledge Shard import uses this to stage and promote verified sidecars
    /// through the same storage root as ordinary attachment operations.
    pub fn filesystem_storage_backend(&self) -> Option<FilesystemBackend> {
        self.file_storage_path.as_ref().map(FilesystemBackend::new)
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
    ///
    /// Verifies PostgreSQL 18+ before running migrations (#635). matric-api
    /// uses RFC 9562 `uuidv7()` and `uuid_extract_timestamp()` built-ins
    /// shipped in PG 18; older versions are not supported.
    #[cfg(feature = "migrations")]
    pub async fn migrate(&self) -> Result<()> {
        self.require_postgres_18().await?;
        self.repair_legacy_migration_history().await?;
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .map_err(|e| Error::Database(sqlx::Error::Migrate(Box::new(e))))?;
        Ok(())
    }

    /// Verify the connected server is PostgreSQL 18 or later (#635).
    ///
    /// Returns an `Error::Config` with a clear remediation message
    /// when the server version is older than 18.0.
    pub async fn require_postgres_18(&self) -> Result<()> {
        let row: (String,) = sqlx::query_as("SHOW server_version_num")
            .fetch_one(&self.pool)
            .await
            .map_err(Error::Database)?;
        let version_num: i32 = row.0.trim().parse().map_err(|_| {
            Error::Config(format!(
                "could not parse PostgreSQL server_version_num={:?}",
                row.0
            ))
        })?;
        if version_num < 180000 {
            return Err(Error::Config(format!(
                "matric-api requires PostgreSQL 18 or later (server_version_num={}). \
                 Install postgresql-18 from PGDG: https://apt.postgresql.org/",
                version_num
            )));
        }
        Ok(())
    }

    /// Normalize known legacy migration history before sqlx validates it.
    ///
    /// A few early 2026 migrations were edited in place or split into
    /// schema-only plus seed-data migrations after they had shipped. sqlx
    /// validates all applied migration checksums before running pending
    /// migrations, so we repair only exact known legacy checksums and mark split
    /// seed migrations as applied only when their data/schema evidence already
    /// exists.
    #[cfg(feature = "migrations")]
    async fn repair_legacy_migration_history(&self) -> Result<()> {
        let has_migrations_table: bool =
            sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
                .fetch_one(&self.pool)
                .await
                .map_err(Error::Database)?;

        if !has_migrations_table {
            return Ok(());
        }

        sqlx::query(
            r#"
            DO $$
            BEGIN
              WITH split_applied(version, description, checksum, source_version, source_sha384) AS (
                VALUES
                  (20260117000002::bigint, 'fix_embedding_set_stats'::text, decode('589b6437386ee16900cf941927a59c0924f23cf9d33e2856acc90782091a945f599fc32b58a7628bbc48d9073a9c8d91', 'hex'), 20260117000001::bigint, '589b6437386ee16900cf941927a59c0924f23cf9d33e2856acc90782091a945f599fc32b58a7628bbc48d9073a9c8d91'),
                  (20260118000001::bigint, 'seed_default_concept_scheme'::text, decode('fc1533fb41e8d4e2042dc196ac0e0dbf43843992b69926724cfee35a82e1aacabeb5b754f62031ef4efc13d9c3e3cbb1', 'hex'), 20260118000000::bigint, '81f588cfbddb017917460ee7d52abdf880d0e55c8faf3cc066698aeed343f1a9bdddb07ef8814c4745ce517f03a02449'),
                  (20260201500001::bigint, 'seed_embedding_configs'::text, decode('862856a288ed2142c635a1d47e5e280e90d5edf6a5a9180eea8a13eb842f4a23226ad65e8137d953f4e2ed0eb6ee7acb', 'hex'), 20260201500000::bigint, '887b598fb59e54e321a894b764a756a7a13fc384b2eba3a9ee9b9e7404e113199de707f0cf765925a63cad5596b54880'),
                  (20260202000001::bigint, 'seed_core_document_types'::text, decode('45a61cef553be5ae3cb2161414cbfb7e6e1bd995ea7b80f91d49df2a5aa28aad85cda44dd0e4e1965867ffcc220284b0', 'hex'), 20260202000000::bigint, 'c386fb8199f53e4d10ee613b6e8729ddfddc332a721cf32107550649a7eb81069a1d9bbee62c484e60a4026e2f52060d'),
                  (20260202100001::bigint, 'embedding_config_api'::text, decode('25271c2b9d8843f8cf3a4a38365da5dfd9e4d477a88421422612775ca27d6dc0c2341566e6950cdc2d89ae3fb9c3c566', 'hex'), 20260202100000::bigint, '25271c2b9d8843f8cf3a4a38365da5dfd9e4d477a88421422612775ca27d6dc0c2341566e6950cdc2d89ae3fb9c3c566'),
                  (20260202100002::bigint, 'seed_agentic_configs'::text, decode('7987079d41542b4cc298a5559c7f9cc84d8a93b26ba3bc081fb99c552b6260ac7f6e115ed737a4c81c0c74e160ae0ef8', 'hex'), 20260202100000::bigint, 'b897a287eddfa2b97ed1a5c54943bd68a4c1274b99dd4de9f2e3175010722200f77f2bdd9d0df222add2ab98128870ee'),
                  (20260202100002::bigint, 'seed_agentic_configs'::text, decode('7987079d41542b4cc298a5559c7f9cc84d8a93b26ba3bc081fb99c552b6260ac7f6e115ed737a4c81c0c74e160ae0ef8', 'hex'), 20260202100000::bigint, '25271c2b9d8843f8cf3a4a38365da5dfd9e4d477a88421422612775ca27d6dc0c2341566e6950cdc2d89ae3fb9c3c566'),
                  (20260203400001::bigint, 'seed_extraction_strategies'::text, decode('3f1a642c1c4a8fee137200a796d714d462a80d20dc6c418e9b7a2e40da9071c6bf9668eca75ee17262cd16dcaf775b33', 'hex'), 20260203400000::bigint, '071780aedfd7163b7e699e412767fa0f805efa3e19d802ac0aa6752bec7a2608894a8eb931d9940a51ac25593978a7b9'),
                  (20260204300001::bigint, 'seed_media_document_types'::text, decode('e3dc1d7da8559017ccb27e5705da20aea8311dff9a5753b4e5ae78914d2c9f45535eb0c02f9cacc43dc0d3188eb21881', 'hex'), 20260204300000::bigint, '3124b8d2f897b6975d13a505d26d521c77d385e3249bceb62e51a561e54797aeb53caedf301c8af07e0625f1e723577f'),
                  (20260204400001::bigint, 'seed_temporal_positional_types'::text, decode('4689fee6bf8a11455edf7d7253405baa2ddfb6080d16a781894052550ffcad1773c0bdd14f12fae6094d287a96414080', 'hex'), 20260204400000::bigint, 'a04dd06d6564f4a3b76507555300c0973cb2ec5b5a05493cd48ce0c0adfb7cae42c216f7efdcde5ff5023014837c1211'),
                  (20260205000001::bigint, 'fix_embedding_pipeline'::text, decode('129709ef75d569f09f6ccd6dd37dcba50c21d0bcf906f56569b9f848cb8cd64a338461f2e3886896989167ddc77d6dec', 'hex'), 20260205000000::bigint, '129709ef75d569f09f6ccd6dd37dcba50c21d0bcf906f56569b9f848cb8cd64a338461f2e3886896989167ddc77d6dec')
              )
              INSERT INTO public._sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
              SELECT split_applied.version,
                     split_applied.description,
                     now(),
                     true,
                     split_applied.checksum,
                     0
                FROM split_applied
               WHERE EXISTS (
                     SELECT 1 FROM public._sqlx_migrations source
                      WHERE source.version = split_applied.source_version
                        AND source.success = true
                        AND source.checksum = decode(split_applied.source_sha384, 'hex')
                   )
                 AND NOT EXISTS (
                     SELECT 1 FROM public._sqlx_migrations existing
                      WHERE existing.version = split_applied.version
                   );

              WITH repairs(version, old_sha384, new_sha384) AS (
                VALUES
                  (20260117000000::bigint, 'c3fdf92e0a59bf1e4d82ac0d85b55e22b99f6466feb567f9d28f5124d3da42bc24c353ae5dfcc453db7e1938d03c1f39', 'f5c978911450c624eefd77ede77aebfa1d3f67f2cd3f57a48d011a79e4dd9c5ccfed82ef0dbb7ea32f58fc0901be9a4b'),
                  (20260117000001::bigint, '589b6437386ee16900cf941927a59c0924f23cf9d33e2856acc90782091a945f599fc32b58a7628bbc48d9073a9c8d91', 'a2cee11a63e0d49e1fafe0a515b036fe0ca1e2278b43c6bc0808c83c8d2236926f8a7bf85bc7a10c66ce80e27639c91d'),
                  (20260118000000::bigint, '81f588cfbddb017917460ee7d52abdf880d0e55c8faf3cc066698aeed343f1a9bdddb07ef8814c4745ce517f03a02449', '54fc55cec2656b3fa3db27c402f7d20ed9b57a2b600fa5fb7c8a50cd044f9a0a80cfdbc14698653db7a985a88266ba7c'),
                  (20260201100000::bigint, 'aa799e6833c44076b0690e714f125ac624a37218c4ba86a29a9d3dab52ff4c27c6f84cddf9993567c5ec4b5512553792', '7c97c54d25900ca247c4085752066667a815a467093778b1e3292fb92361632e910e99574ebb012b3cd16a32ae8020cd'),
                  (20260201200000::bigint, '2da68242b4486d9e04dd6eec9c7c25f29340f7f37ac87007756fd4a938317d7e6b1d84f781f298eb73b84896dd75f80d', 'd5de77042eaca3f3e8108c440eb93bf9f11afba15dd5fcf3b33405cdb89ff3d65ab5473a0de63442e4cd280b64bbc867'),
                  (20260201300000::bigint, '5aced078ba7b53c5a6d370a9b840d51ac2e1cfd813f95b0efc37895167031c853f077c169a28e1c8e6e4311c941d1a5e', '75236d37a5ccf3e067556c96b76d07102a42b214633f3aad20fbf719dcb7e540cfcb08c0cdd346ace02eae4fc33f1e04'),
                  (20260201500000::bigint, '887b598fb59e54e321a894b764a756a7a13fc384b2eba3a9ee9b9e7404e113199de707f0cf765925a63cad5596b54880', '9deb4f9fd460e2dc455174698b574d0f77730d65366f9ffb4a5604cf759284fe7f05572b5ca6db3453e8ccd5cc8625a4'),
                  (20260202000000::bigint, 'c386fb8199f53e4d10ee613b6e8729ddfddc332a721cf32107550649a7eb81069a1d9bbee62c484e60a4026e2f52060d', 'db891ca0c1b00bff3a1162409b67e1a1d3109cbba1bcaf0dfd8e28063b20d2db616c9734607fa3ff9bd8b4cdd5d2f8e4'),
                  (20260202100000::bigint, 'b897a287eddfa2b97ed1a5c54943bd68a4c1274b99dd4de9f2e3175010722200f77f2bdd9d0df222add2ab98128870ee', '75aa015c0ef47fd2f99692455ff095cb56eb5006a1bde99278980676252d5b613c20baf0b01a0ccdae95c41dcfd1e5ce'),
                  (20260202100000::bigint, '25271c2b9d8843f8cf3a4a38365da5dfd9e4d477a88421422612775ca27d6dc0c2341566e6950cdc2d89ae3fb9c3c566', '75aa015c0ef47fd2f99692455ff095cb56eb5006a1bde99278980676252d5b613c20baf0b01a0ccdae95c41dcfd1e5ce'),
                  (20260203400000::bigint, '071780aedfd7163b7e699e412767fa0f805efa3e19d802ac0aa6752bec7a2608894a8eb931d9940a51ac25593978a7b9', '8744a8e7e5aca7caf6cc5ce4073ee1d7f374b76012e16cd53ef41b1fed9f0c9860c66147ae5941a1471103135296e660'),
                  (20260204100000::bigint, 'c39e18dfa22ed2bc8c637fbf234024feb8ca957cad43de3bbff60d400e7b841067b9e352af65c928d28a177777e3ed36', '4feb4008a64f1a2fc9143e62950bf71d080f1d793a84ed3151a25912c17b62954500e92a08fa1edc63faedb8d0247062'),
                  (20260204300000::bigint, '3124b8d2f897b6975d13a505d26d521c77d385e3249bceb62e51a561e54797aeb53caedf301c8af07e0625f1e723577f', 'f1ca1202710adb96d33fdb44b954997ba2d2562758d295dd6267dd698010218c33bbd2f21cb8d9718235feba03b93eb4'),
                  (20260204400000::bigint, 'a04dd06d6564f4a3b76507555300c0973cb2ec5b5a05493cd48ce0c0adfb7cae42c216f7efdcde5ff5023014837c1211', '6cd49a208083d58085e8f56ae965472549b003895bddfe19995a234013e8e80b12315796bbc2d85538ea71a62c49237b'),
                  (20260205000000::bigint, '129709ef75d569f09f6ccd6dd37dcba50c21d0bcf906f56569b9f848cb8cd64a338461f2e3886896989167ddc77d6dec', '299534b2f551486fc188eddc173f58088fe00823afffbc76f2f9bc44bc558ca2fb6c337c5a40158b3e2f43c5a5d65e1a'),
                  (20260215000000::bigint, '2bdad6ec8fffbe68cde85e0e749ac510ef319b694aa15dee71bcae3ad13b3db2f8b317f7ef2b393ea27e432b5f33872c', 'c4a8d7097ce200e9bd39d7bd70882403119c1181bbfa5999335d48ebd087e9703587297347bbef014974cb1699f07772')
              )
              UPDATE public._sqlx_migrations AS migration
                 SET checksum = decode(repairs.new_sha384, 'hex')
                FROM repairs
               WHERE migration.version = repairs.version
                 AND migration.success = true
                 AND migration.checksum = decode(repairs.old_sha384, 'hex');
            END $$;
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

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
            job_notify: self.job_notify.clone(),
            notes: PgNoteRepository::new(self.pool.clone()),
            embeddings: PgEmbeddingRepository::new(self.pool.clone()),
            embedding_sets: PgEmbeddingSetRepository::new(self.pool.clone()),
            links: PgLinkRepository::new(self.pool.clone()),
            tags: PgTagRepository::new(self.pool.clone()),
            skos: skos.clone(),
            collections: PgCollectionRepository::new(self.pool.clone()),
            document_types: PgDocumentTypeRepository::new(self.pool.clone()),
            jobs: PgJobRepository::with_notify(self.pool.clone(), self.job_notify.clone()),
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
            incoming_webhooks: PgIncomingWebhookReceiverRepository::new(self.pool.clone()),
            inbound_sources: PgInboundSourceRepository::new(self.pool.clone()),
            outbox: PgEventOutboxRepository::new(self.pool.clone()),
            pke_keys: PgPkeKeyRepository::new(self.pool.clone()),
            pke_keysets: PgPkeKeysetRepository::new(self.pool.clone()),
            tus: PgTusRepository::new(self.pool.clone()),
            call_sessions: PgCallSessionRepository::new(self.pool.clone()),
        }
    }
}
