//! Hybrid search combining FTS and semantic vector search.

use async_trait::async_trait;
use pgvector::Vector;
use uuid::Uuid;

use matric_core::{EmbeddingRepository, Result, SearchHit};
use matric_db::Database;

use crate::rrf::rrf_fuse;

/// Configuration for hybrid search.
#[derive(Debug, Clone)]
pub struct HybridSearchConfig {
    /// Weight for FTS results (0.0 to 1.0)
    pub fts_weight: f32,
    /// Weight for semantic results (0.0 to 1.0)
    pub semantic_weight: f32,
    /// Whether to exclude archived notes
    pub exclude_archived: bool,
    /// Minimum score threshold (0.0 to 1.0)
    pub min_score: f32,
    /// Optional embedding set to search within (None = default/all embeddings)
    pub embedding_set_id: Option<Uuid>,
}

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            fts_weight: 0.5,
            semantic_weight: 0.5,
            exclude_archived: true,
            min_score: 0.0,
            embedding_set_id: None,
        }
    }
}

impl HybridSearchConfig {
    /// Create a new config with custom weights.
    pub fn with_weights(fts_weight: f32, semantic_weight: f32) -> Self {
        Self {
            fts_weight,
            semantic_weight,
            ..Default::default()
        }
    }

    /// Create a config for FTS-only search.
    pub fn fts_only() -> Self {
        Self {
            fts_weight: 1.0,
            semantic_weight: 0.0,
            ..Default::default()
        }
    }

    /// Create a config for semantic-only search.
    pub fn semantic_only() -> Self {
        Self {
            fts_weight: 0.0,
            semantic_weight: 1.0,
            ..Default::default()
        }
    }

    /// Set minimum score threshold.
    pub fn with_min_score(mut self, min_score: f32) -> Self {
        self.min_score = min_score;
        self
    }

    /// Set whether to exclude archived notes.
    pub fn with_exclude_archived(mut self, exclude: bool) -> Self {
        self.exclude_archived = exclude;
        self
    }

    /// Set embedding set to search within.
    pub fn with_embedding_set(mut self, set_id: Uuid) -> Self {
        self.embedding_set_id = Some(set_id);
        self
    }
}

/// Trait for hybrid search operations.
#[async_trait]
pub trait HybridSearch: Send + Sync {
    /// Perform hybrid search with text query and optional embedding.
    async fn search(
        &self,
        query: &str,
        query_embedding: Option<&Vector>,
        limit: i64,
        config: &HybridSearchConfig,
    ) -> Result<Vec<SearchHit>>;

    /// Perform filtered hybrid search.
    async fn search_filtered(
        &self,
        query: &str,
        query_embedding: Option<&Vector>,
        filters: &str,
        limit: i64,
        config: &HybridSearchConfig,
    ) -> Result<Vec<SearchHit>>;

    /// Find similar notes by embedding only.
    async fn find_similar(
        &self,
        query_embedding: &Vector,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>>;

    /// Find similar notes within a specific embedding set.
    async fn find_similar_in_set(
        &self,
        query_embedding: &Vector,
        embedding_set_id: Uuid,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>>;

    /// Search by keyword (FTS only).
    async fn search_by_keyword(&self, term: &str, limit: i64) -> Result<Vec<Uuid>>;
}

/// Hybrid search engine implementation.
pub struct HybridSearchEngine {
    db: Database,
}

impl HybridSearchEngine {
    /// Create a new hybrid search engine.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Apply score weighting to search results.
    fn apply_weights(hits: Vec<SearchHit>, weight: f32) -> Vec<SearchHit> {
        hits.into_iter()
            .map(|mut hit| {
                hit.score *= weight;
                hit
            })
            .collect()
    }
}

#[async_trait]
impl HybridSearch for HybridSearchEngine {
    async fn search(
        &self,
        query: &str,
        query_embedding: Option<&Vector>,
        limit: i64,
        config: &HybridSearchConfig,
    ) -> Result<Vec<SearchHit>> {
        let mut ranked_lists = Vec::new();

        // FTS search (if weight > 0 and query is not empty)
        if config.fts_weight > 0.0 && !query.trim().is_empty() {
            let fts_results = self
                .db
                .search
                .search(query, limit * 2, config.exclude_archived)
                .await?;

            if !fts_results.is_empty() {
                ranked_lists.push(Self::apply_weights(fts_results, config.fts_weight));
            }
        }

        // Semantic search (if weight > 0 and embedding is provided)
        if config.semantic_weight > 0.0 {
            if let Some(embedding) = query_embedding {
                // Use embedding set if specified, otherwise search all embeddings
                let semantic_results = if let Some(set_id) = config.embedding_set_id {
                    self.db
                        .embeddings
                        .find_similar_in_set(embedding, set_id, limit * 2, config.exclude_archived)
                        .await?
                } else {
                    self.db
                        .embeddings
                        .find_similar(embedding, limit * 2, config.exclude_archived)
                        .await?
                };

                if !semantic_results.is_empty() {
                    ranked_lists.push(Self::apply_weights(
                        semantic_results,
                        config.semantic_weight,
                    ));
                }
            }
        }

        // If no results from either source, return empty
        if ranked_lists.is_empty() {
            return Ok(Vec::new());
        }

        // Fuse results using RRF
        let mut results = rrf_fuse(ranked_lists, limit as usize);

        // Apply minimum score filter
        if config.min_score > 0.0 {
            results.retain(|hit| hit.score >= config.min_score);
        }

        Ok(results)
    }

    async fn search_filtered(
        &self,
        query: &str,
        query_embedding: Option<&Vector>,
        filters: &str,
        limit: i64,
        config: &HybridSearchConfig,
    ) -> Result<Vec<SearchHit>> {
        let mut ranked_lists = Vec::new();

        // FTS search with filters
        if config.fts_weight > 0.0 && !query.trim().is_empty() {
            let fts_results = self
                .db
                .search
                .search_filtered(query, filters, limit * 2, config.exclude_archived)
                .await?;

            if !fts_results.is_empty() {
                ranked_lists.push(Self::apply_weights(fts_results, config.fts_weight));
            }
        }

        // Semantic search (filters not applied to vector search - we filter after fusion)
        if config.semantic_weight > 0.0 {
            if let Some(embedding) = query_embedding {
                // Use embedding set if specified, otherwise search all embeddings
                let semantic_results = if let Some(set_id) = config.embedding_set_id {
                    self.db
                        .embeddings
                        .find_similar_in_set(embedding, set_id, limit * 2, config.exclude_archived)
                        .await?
                } else {
                    self.db
                        .embeddings
                        .find_similar(embedding, limit * 2, config.exclude_archived)
                        .await?
                };

                if !semantic_results.is_empty() {
                    ranked_lists.push(Self::apply_weights(
                        semantic_results,
                        config.semantic_weight,
                    ));
                }
            }
        }

        if ranked_lists.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = rrf_fuse(ranked_lists, limit as usize);

        if config.min_score > 0.0 {
            results.retain(|hit| hit.score >= config.min_score);
        }

        Ok(results)
    }

    async fn find_similar(
        &self,
        query_embedding: &Vector,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        self.db
            .embeddings
            .find_similar(query_embedding, limit, exclude_archived)
            .await
    }

    async fn find_similar_in_set(
        &self,
        query_embedding: &Vector,
        embedding_set_id: Uuid,
        limit: i64,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        self.db
            .embeddings
            .find_similar_in_set(query_embedding, embedding_set_id, limit, exclude_archived)
            .await
    }

    async fn search_by_keyword(&self, term: &str, limit: i64) -> Result<Vec<Uuid>> {
        self.db.search.search_by_keyword(term, limit).await
    }
}

/// Builder for creating hybrid search requests.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    query: String,
    embedding: Option<Vector>,
    filters: Option<String>,
    limit: i64,
    config: HybridSearchConfig,
    /// Filter: notes created after this timestamp
    created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes created before this timestamp
    created_before: Option<chrono::DateTime<chrono::Utc>>,
}

impl SearchRequest {
    /// Create a new search request with a text query.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            embedding: None,
            filters: None,
            limit: 20,
            config: HybridSearchConfig::default(),
            created_after: None,
            created_before: None,
        }
    }

    /// Filter to notes created after this timestamp.
    pub fn with_created_after(mut self, ts: chrono::DateTime<chrono::Utc>) -> Self {
        self.created_after = Some(ts);
        self
    }

    /// Filter to notes created before this timestamp.
    pub fn with_created_before(mut self, ts: chrono::DateTime<chrono::Utc>) -> Self {
        self.created_before = Some(ts);
        self
    }

    /// Add an embedding for semantic search.
    pub fn with_embedding(mut self, embedding: Vector) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Add filters (e.g., "tag:rust collection:uuid").
    pub fn with_filters(mut self, filters: impl Into<String>) -> Self {
        self.filters = Some(filters.into());
        self
    }

    /// Set the result limit.
    pub fn with_limit(mut self, limit: i64) -> Self {
        self.limit = limit;
        self
    }

    /// Set the search configuration.
    pub fn with_config(mut self, config: HybridSearchConfig) -> Self {
        self.config = config;
        self
    }

    /// Set FTS-only mode.
    pub fn fts_only(mut self) -> Self {
        self.config = HybridSearchConfig::fts_only();
        self
    }

    /// Set semantic-only mode.
    pub fn semantic_only(mut self) -> Self {
        self.config = HybridSearchConfig::semantic_only();
        self
    }

    /// Restrict semantic search to a specific embedding set.
    pub fn with_embedding_set(mut self, set_id: Uuid) -> Self {
        self.config.embedding_set_id = Some(set_id);
        self
    }

    /// Execute the search request.
    pub async fn execute(self, engine: &HybridSearchEngine) -> Result<Vec<SearchHit>> {
        // Build filters string with temporal filters
        let mut filter_parts: Vec<String> = Vec::new();
        if let Some(f) = &self.filters {
            filter_parts.push(f.clone());
        }
        if let Some(ts) = &self.created_after {
            filter_parts.push(format!("created_after:{}", ts.to_rfc3339()));
        }
        if let Some(ts) = &self.created_before {
            filter_parts.push(format!("created_before:{}", ts.to_rfc3339()));
        }

        if !filter_parts.is_empty() {
            let combined_filters = filter_parts.join(" ");
            engine
                .search_filtered(
                    &self.query,
                    self.embedding.as_ref(),
                    &combined_filters,
                    self.limit,
                    &self.config,
                )
                .await
        } else {
            engine
                .search(
                    &self.query,
                    self.embedding.as_ref(),
                    self.limit,
                    &self.config,
                )
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = HybridSearchConfig::default();
        assert_eq!(config.fts_weight, 0.5);
        assert_eq!(config.semantic_weight, 0.5);
        assert!(config.exclude_archived);
        assert_eq!(config.min_score, 0.0);
        assert!(config.embedding_set_id.is_none());
    }

    #[test]
    fn test_config_fts_only() {
        let config = HybridSearchConfig::fts_only();
        assert_eq!(config.fts_weight, 1.0);
        assert_eq!(config.semantic_weight, 0.0);
    }

    #[test]
    fn test_config_semantic_only() {
        let config = HybridSearchConfig::semantic_only();
        assert_eq!(config.fts_weight, 0.0);
        assert_eq!(config.semantic_weight, 1.0);
    }

    #[test]
    fn test_apply_weights() {
        let hits = vec![
            SearchHit {
                note_id: Uuid::new_v4(),
                score: 1.0,
                snippet: None,
                title: None,
                tags: Vec::new(),
            },
            SearchHit {
                note_id: Uuid::new_v4(),
                score: 0.5,
                snippet: None,
                title: None,
                tags: Vec::new(),
            },
        ];

        let weighted = HybridSearchEngine::apply_weights(hits, 0.5);
        assert_eq!(weighted[0].score, 0.5);
        assert_eq!(weighted[1].score, 0.25);
    }

    #[test]
    fn test_search_request_builder() {
        let request = SearchRequest::new("test query")
            .with_limit(10)
            .with_filters("tag:rust")
            .fts_only();

        assert_eq!(request.query, "test query");
        assert_eq!(request.limit, 10);
        assert_eq!(request.filters, Some("tag:rust".to_string()));
        assert_eq!(request.config.fts_weight, 1.0);
        assert_eq!(request.config.semantic_weight, 0.0);
    }

    #[test]
    fn test_config_with_embedding_set() {
        let set_id = Uuid::new_v4();
        let config = HybridSearchConfig::default().with_embedding_set(set_id);
        assert_eq!(config.embedding_set_id, Some(set_id));
    }

    #[test]
    fn test_search_request_with_embedding_set() {
        let set_id = Uuid::new_v4();
        let request = SearchRequest::new("test query")
            .semantic_only()
            .with_embedding_set(set_id);

        assert_eq!(request.config.embedding_set_id, Some(set_id));
        assert_eq!(request.config.semantic_weight, 1.0);
        assert_eq!(request.config.fts_weight, 0.0);
    }

    // ========== NEW COMPREHENSIVE TESTS ==========

    #[test]
    fn test_config_with_weights_custom_values() {
        let config = HybridSearchConfig::with_weights(0.7, 0.3);
        assert_eq!(config.fts_weight, 0.7);
        assert_eq!(config.semantic_weight, 0.3);
        // Ensure other defaults are preserved
        assert!(config.exclude_archived);
        assert_eq!(config.min_score, 0.0);
        assert!(config.embedding_set_id.is_none());
    }

    #[test]
    fn test_config_with_weights_boundary_values() {
        let config1 = HybridSearchConfig::with_weights(0.0, 1.0);
        assert_eq!(config1.fts_weight, 0.0);
        assert_eq!(config1.semantic_weight, 1.0);

        let config2 = HybridSearchConfig::with_weights(1.0, 0.0);
        assert_eq!(config2.fts_weight, 1.0);
        assert_eq!(config2.semantic_weight, 0.0);
    }

    #[test]
    fn test_config_with_min_score() {
        let config = HybridSearchConfig::default().with_min_score(0.5);
        assert_eq!(config.min_score, 0.5);
    }

    #[test]
    fn test_config_with_min_score_boundary_values() {
        let config1 = HybridSearchConfig::default().with_min_score(0.0);
        assert_eq!(config1.min_score, 0.0);

        let config2 = HybridSearchConfig::default().with_min_score(1.0);
        assert_eq!(config2.min_score, 1.0);
    }

    #[test]
    fn test_config_with_exclude_archived_true() {
        let config = HybridSearchConfig::default().with_exclude_archived(true);
        assert!(config.exclude_archived);
    }

    #[test]
    fn test_config_with_exclude_archived_false() {
        let config = HybridSearchConfig::default().with_exclude_archived(false);
        assert!(!config.exclude_archived);
    }

    #[test]
    fn test_config_chaining_multiple_builders() {
        let set_id = Uuid::new_v4();
        let config = HybridSearchConfig::default()
            .with_min_score(0.3)
            .with_exclude_archived(false)
            .with_embedding_set(set_id);

        assert_eq!(config.min_score, 0.3);
        assert!(!config.exclude_archived);
        assert_eq!(config.embedding_set_id, Some(set_id));
    }

    #[test]
    fn test_apply_weights_with_zero_weight() {
        let hits = vec![SearchHit {
            note_id: Uuid::new_v4(),
            score: 1.0,
            snippet: None,
            title: None,
            tags: Vec::new(),
        }];

        let weighted = HybridSearchEngine::apply_weights(hits, 0.0);
        assert_eq!(weighted[0].score, 0.0);
    }

    #[test]
    fn test_apply_weights_with_full_weight() {
        let hits = vec![SearchHit {
            note_id: Uuid::new_v4(),
            score: 0.8,
            snippet: None,
            title: None,
            tags: Vec::new(),
        }];

        let weighted = HybridSearchEngine::apply_weights(hits, 1.0);
        assert_eq!(weighted[0].score, 0.8);
    }

    #[test]
    fn test_apply_weights_empty_list() {
        let hits: Vec<SearchHit> = vec![];
        let weighted = HybridSearchEngine::apply_weights(hits, 0.5);
        assert_eq!(weighted.len(), 0);
    }

    #[test]
    fn test_apply_weights_preserves_metadata() {
        let hits = vec![SearchHit {
            note_id: Uuid::new_v4(),
            score: 1.0,
            snippet: Some("test snippet".to_string()),
            title: Some("Test Title".to_string()),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
        }];

        let weighted = HybridSearchEngine::apply_weights(hits.clone(), 0.5);
        assert_eq!(weighted[0].snippet, hits[0].snippet);
        assert_eq!(weighted[0].title, hits[0].title);
        assert_eq!(weighted[0].tags, hits[0].tags);
        assert_eq!(weighted[0].note_id, hits[0].note_id);
    }

    #[test]
    fn test_search_request_new() {
        let request = SearchRequest::new("test query");
        assert_eq!(request.query, "test query");
        assert_eq!(request.limit, 20);
        assert!(request.embedding.is_none());
        assert!(request.filters.is_none());
        assert!(request.created_after.is_none());
        assert!(request.created_before.is_none());
    }

    #[test]
    fn test_search_request_with_created_after() {
        use chrono::Utc;
        let ts = Utc::now();
        let request = SearchRequest::new("test").with_created_after(ts);
        assert_eq!(request.created_after, Some(ts));
    }

    #[test]
    fn test_search_request_with_created_before() {
        use chrono::Utc;
        let ts = Utc::now();
        let request = SearchRequest::new("test").with_created_before(ts);
        assert_eq!(request.created_before, Some(ts));
    }

    #[test]
    fn test_search_request_with_embedding() {
        let embedding = Vector::from(vec![0.1, 0.2, 0.3]);
        let request = SearchRequest::new("test").with_embedding(embedding.clone());
        assert!(request.embedding.is_some());
    }

    #[test]
    fn test_search_request_with_config() {
        let config = HybridSearchConfig::fts_only();
        let request = SearchRequest::new("test").with_config(config.clone());
        assert_eq!(request.config.fts_weight, config.fts_weight);
        assert_eq!(request.config.semantic_weight, config.semantic_weight);
    }

    #[test]
    fn test_search_request_semantic_only() {
        let request = SearchRequest::new("test").semantic_only();
        assert_eq!(request.config.fts_weight, 0.0);
        assert_eq!(request.config.semantic_weight, 1.0);
    }

    #[test]
    fn test_search_request_chaining_all_options() {
        use chrono::Utc;
        let ts_after = Utc::now();
        let ts_before = Utc::now();
        let set_id = Uuid::new_v4();
        let embedding = Vector::from(vec![0.1, 0.2]);

        let request = SearchRequest::new("complex query")
            .with_limit(50)
            .with_filters("tag:rust collection:test")
            .with_embedding(embedding)
            .with_created_after(ts_after)
            .with_created_before(ts_before)
            .with_embedding_set(set_id);

        assert_eq!(request.query, "complex query");
        assert_eq!(request.limit, 50);
        assert_eq!(
            request.filters,
            Some("tag:rust collection:test".to_string())
        );
        assert!(request.embedding.is_some());
        assert_eq!(request.created_after, Some(ts_after));
        assert_eq!(request.created_before, Some(ts_before));
        assert_eq!(request.config.embedding_set_id, Some(set_id));
    }

    #[test]
    fn test_search_request_with_limit_boundary() {
        let request = SearchRequest::new("test").with_limit(1);
        assert_eq!(request.limit, 1);
    }

    #[test]
    fn test_search_request_with_limit_large() {
        let request = SearchRequest::new("test").with_limit(1000);
        assert_eq!(request.limit, 1000);
    }
}
