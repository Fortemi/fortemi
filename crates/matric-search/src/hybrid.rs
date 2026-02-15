//! Hybrid search combining FTS and semantic vector search.
//!
//! Supports multilingual search via:
//! - Script detection for automatic search strategy selection
//! - Language hints for explicit routing
//! - Trigram/bigram fallback for CJK, emoji, and symbols

use std::time::Instant;

use async_trait::async_trait;
use pgvector::Vector;
use tracing::{debug, info, instrument};
use uuid::Uuid;

use matric_core::{EmbeddingRepository, Result, SearchHit, StrictFilter, StrictTagFilter};
use matric_db::Database;

use crate::deduplication::{deduplicate_search_results, DeduplicationConfig, EnhancedSearchHit};
use crate::fts_flags::FtsFeatureFlags;
use crate::rrf::rrf_fuse;
use crate::script_detection::{detect_script, DetectedScript};

/// Minimum raw cosine similarity for semantic results to enter RRF fusion.
///
/// Semantic search always returns top-K results regardless of actual similarity.
/// Without this threshold, nonsense queries produce inflated RRF scores because
/// single-list normalization maps even low-similarity results to near 1.0.
///
/// 0.3 is conservative — typical good matches score 0.5-0.9, while truly
/// unrelated content scores below 0.2. This filters noise without losing
/// marginal but potentially useful results.
const MIN_SEMANTIC_SIMILARITY: f32 = 0.3;

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
    /// Deduplication configuration for handling chunked documents
    pub deduplication: DeduplicationConfig,
    /// Strict tag filter for precise taxonomy-based filtering (legacy - prefer unified_filter)
    pub strict_filter: Option<StrictTagFilter>,
    /// Unified strict filter for multi-dimensional filtering.
    /// When set, takes precedence over strict_filter for FTS.
    pub unified_filter: Option<StrictFilter>,
    /// Optional ISO 639-1 language hint (e.g., "en", "zh", "ja", "de")
    pub lang_hint: Option<String>,
    /// Optional script hint (e.g., "latin", "han", "cyrillic")
    pub script_hint: Option<String>,
    /// Feature flags for multilingual search (controls which features are enabled)
    pub fts_flags: FtsFeatureFlags,
}

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            fts_weight: 0.5,
            semantic_weight: 0.5,
            exclude_archived: true,
            min_score: 0.0,
            embedding_set_id: None,
            deduplication: DeduplicationConfig::default(),
            strict_filter: None,
            unified_filter: None,
            lang_hint: None,
            script_hint: None,
            fts_flags: FtsFeatureFlags::default(),
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

    /// Set deduplication configuration.
    pub fn with_deduplication(mut self, config: DeduplicationConfig) -> Self {
        self.deduplication = config;
        self
    }

    /// Enable deduplication (on by default).
    pub fn with_deduplication_enabled(mut self, enabled: bool) -> Self {
        self.deduplication.deduplicate_chains = enabled;
        self
    }

    /// Enable chain expansion.
    pub fn with_chain_expansion(mut self, expand: bool) -> Self {
        self.deduplication.expand_chains = expand;
        self
    }

    /// Set strict tag filter for taxonomy-based filtering.
    pub fn with_strict_filter(mut self, filter: StrictTagFilter) -> Self {
        self.strict_filter = Some(filter);
        self
    }

    /// Set unified strict filter for multi-dimensional filtering.
    /// This takes precedence over strict_filter when both are set.
    pub fn with_unified_filter(mut self, filter: StrictFilter) -> Self {
        self.unified_filter = Some(filter);
        self
    }

    /// Set ISO 639-1 language hint (e.g., "en", "zh", "ja", "de").
    /// Overrides automatic script detection.
    pub fn with_lang_hint(mut self, lang: impl Into<String>) -> Self {
        self.lang_hint = Some(lang.into());
        self
    }

    /// Set script hint (e.g., "latin", "han", "cyrillic").
    /// Overrides automatic script detection.
    pub fn with_script_hint(mut self, script: impl Into<String>) -> Self {
        self.script_hint = Some(script.into());
        self
    }

    /// Set feature flags for multilingual search.
    pub fn with_fts_flags(mut self, flags: FtsFeatureFlags) -> Self {
        self.fts_flags = flags;
        self
    }
}

/// Trait for hybrid search operations.
#[async_trait]
pub trait HybridSearch: Send + Sync {
    /// Perform hybrid search with text query and optional embedding.
    /// Returns enhanced search results with deduplication and chain info.
    async fn search(
        &self,
        query: &str,
        query_embedding: Option<&Vector>,
        limit: i64,
        config: &HybridSearchConfig,
    ) -> Result<Vec<EnhancedSearchHit>>;

    /// Perform filtered hybrid search.
    /// Returns enhanced search results with deduplication and chain info.
    async fn search_filtered(
        &self,
        query: &str,
        query_embedding: Option<&Vector>,
        filters: &str,
        limit: i64,
        config: &HybridSearchConfig,
    ) -> Result<Vec<EnhancedSearchHit>>;

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

/// Search strategy based on script detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStrategy {
    /// Standard FTS with matric_english (default for Latin)
    FtsEnglish,
    /// Simple FTS without stemming (for unknown languages)
    FtsSimple,
    /// Trigram-based search (for emoji, symbols, fallback)
    Trigram,
    /// Bigram-based search (optimized for CJK)
    Bigram,
    /// Combined CJK strategy (bigram if available, else trigram)
    Cjk,
}

/// Metadata about the search operation.
#[derive(Debug, Clone)]
pub struct SearchMetadata {
    /// Detected script of the query
    pub detected_script: DetectedScript,
    /// Search strategy used
    pub strategy: SearchStrategy,
    /// FTS configuration used (if applicable)
    pub fts_config: Option<String>,
    /// Number of FTS hits
    pub fts_hits: usize,
    /// Number of semantic hits
    pub semantic_hits: usize,
    /// Total search time in milliseconds
    pub search_time_ms: u64,
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

    /// Get note IDs that belong to an embedding set (for FTS post-filtering, issue #125).
    async fn get_set_member_ids(&self, set_id: Uuid) -> Result<std::collections::HashSet<Uuid>> {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT note_id FROM embedding_set_member WHERE embedding_set_id = $1",
        )
        .bind(set_id)
        .fetch_all(self.db.pool())
        .await
        .map_err(matric_core::Error::Database)?;

        Ok(ids.into_iter().collect())
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

    /// Select search strategy based on query script and feature flags.
    fn select_strategy(
        query: &str,
        config: &HybridSearchConfig,
    ) -> (SearchStrategy, DetectedScript) {
        // Check if script detection is enabled
        if !config.fts_flags.script_detection {
            return (SearchStrategy::FtsEnglish, DetectedScript::Latin);
        }

        // Script hint takes priority
        if let Some(ref script) = config.script_hint {
            let detected = match script.to_lowercase().as_str() {
                "han" | "cjk" | "chinese" | "japanese" | "korean" => DetectedScript::Cjk,
                "cyrillic" | "russian" => DetectedScript::Cyrillic,
                "arabic" => DetectedScript::Arabic,
                "latin" | "english" => DetectedScript::Latin,
                _ => detect_script(query).primary,
            };
            return (Self::strategy_for_script(&detected, config), detected);
        }

        // Language hint
        if let Some(ref lang) = config.lang_hint {
            let detected = match lang.to_lowercase().as_str() {
                "zh" | "ja" | "ko" => DetectedScript::Cjk,
                "ru" => DetectedScript::Cyrillic,
                "ar" => DetectedScript::Arabic,
                "en" | "de" | "fr" | "es" | "pt" => DetectedScript::Latin,
                _ => detect_script(query).primary,
            };
            return (Self::strategy_for_script(&detected, config), detected);
        }

        // Auto-detect script
        let detection = detect_script(query);
        let strategy = Self::strategy_for_script(&detection.primary, config);
        (strategy, detection.primary)
    }

    /// Get search strategy for a detected script.
    fn strategy_for_script(script: &DetectedScript, config: &HybridSearchConfig) -> SearchStrategy {
        match script {
            DetectedScript::Latin => SearchStrategy::FtsEnglish,
            DetectedScript::Cjk => {
                if config.fts_flags.bigram_cjk {
                    SearchStrategy::Bigram
                } else if config.fts_flags.trigram_fallback {
                    SearchStrategy::Cjk
                } else {
                    SearchStrategy::FtsSimple
                }
            }
            DetectedScript::Emoji | DetectedScript::Symbol => {
                // Emoji and math symbols require trigram search - FTS doesn't tokenize them
                if config.fts_flags.trigram_fallback {
                    SearchStrategy::Trigram
                } else {
                    SearchStrategy::FtsSimple
                }
            }
            DetectedScript::Cyrillic
            | DetectedScript::Arabic
            | DetectedScript::Greek
            | DetectedScript::Hebrew
            | DetectedScript::Devanagari
            | DetectedScript::Thai => {
                // TODO: When multilingual_configs is true, could select language-specific config
                // based on detected script (e.g., matric_russian for Cyrillic)
                SearchStrategy::FtsSimple
            }
            DetectedScript::Mixed => {
                // For mixed scripts, use trigram if available for best coverage
                if config.fts_flags.trigram_fallback {
                    SearchStrategy::Trigram
                } else {
                    SearchStrategy::FtsSimple
                }
            }
            DetectedScript::Unknown => SearchStrategy::FtsSimple,
        }
    }

    /// Perform FTS search with the appropriate strategy.
    ///
    /// Applies strict_filter for all strategies: English FTS uses server-side SQL
    /// filtering; non-English strategies use post-filtering (fixes #235, #236).
    async fn fts_search_with_strategy(
        &self,
        query: &str,
        strategy: SearchStrategy,
        limit: i64,
        config: &HybridSearchConfig,
    ) -> Result<Vec<SearchHit>> {
        let mut results = match strategy {
            SearchStrategy::FtsEnglish => {
                if let Some(ref strict_filter) = config.strict_filter {
                    self.db
                        .search
                        .search_with_strict_filter(
                            query,
                            Some(strict_filter),
                            limit,
                            config.exclude_archived,
                        )
                        .await?
                } else {
                    self.db
                        .search
                        .search(query, limit, config.exclude_archived)
                        .await?
                }
            }
            SearchStrategy::FtsSimple => {
                self.db
                    .search
                    .search_simple(query, limit, config.exclude_archived)
                    .await?
            }
            SearchStrategy::Trigram => {
                self.db
                    .search
                    .search_trigram(query, limit, config.exclude_archived)
                    .await?
            }
            SearchStrategy::Bigram => {
                self.db
                    .search
                    .search_bigram(query, limit, config.exclude_archived)
                    .await?
            }
            SearchStrategy::Cjk => {
                self.db
                    .search
                    .search_cjk(query, limit, config.exclude_archived)
                    .await?
            }
        };

        // Post-filter by strict_filter for non-English strategies (fixes #236).
        // English FTS handles this server-side via search_with_strict_filter.
        if strategy != SearchStrategy::FtsEnglish {
            if let Some(ref strict_filter) = config.strict_filter {
                if strict_filter.match_none {
                    return Ok(Vec::new());
                }
                if !strict_filter.is_empty() {
                    let note_ids: Vec<Uuid> = results.iter().map(|h| h.note_id).collect();
                    if !note_ids.is_empty() {
                        let matching = self
                            .filter_notes_by_strict_filter(&note_ids, strict_filter)
                            .await?;
                        results.retain(|hit| matching.contains(&hit.note_id));
                    }
                }
            }
        }

        Ok(results)
    }

    /// Post-filter FTS results by query-level filters (tag, collection, temporal).
    ///
    /// Used when non-English FTS strategies (trigram, bigram, CJK) are selected
    /// for search_filtered — these strategies don't have SQL-level filter variants,
    /// so we get unfiltered FTS results and filter them here.
    async fn post_filter_by_query_filters(
        &self,
        results: Vec<SearchHit>,
        filters: &str,
        exclude_archived: bool,
    ) -> Result<Vec<SearchHit>> {
        if results.is_empty() || filters.trim().is_empty() {
            return Ok(results);
        }

        let note_ids: Vec<Uuid> = results.iter().map(|h| h.note_id).collect();

        let archive_clause = if exclude_archived {
            "AND (n.archived IS FALSE OR n.archived IS NULL) AND n.deleted_at IS NULL"
        } else {
            "AND n.deleted_at IS NULL"
        };

        let mut sql = format!(
            "SELECT n.id FROM note n WHERE n.id = ANY($1::uuid[]) {}",
            archive_clause
        );
        let mut params: Vec<String> = Vec::new();

        for token in filters.split_whitespace() {
            if let Some(tag) = token.strip_prefix("tag:") {
                params.push(tag.to_string());
                let exact_idx = params.len() + 1; // +1 because $1 is note_ids
                params.push(matric_db::escape_like(tag));
                let like_idx = params.len() + 1;
                sql.push_str(&format!(
                    " AND n.id IN (SELECT note_id FROM note_tag WHERE LOWER(tag_name) = LOWER(${exact_idx}::text) OR LOWER(tag_name) LIKE LOWER(${like_idx}::text) || '/%' ESCAPE '\\\\')",
                ));
            } else if let Some(collection) = token.strip_prefix("collection:") {
                if uuid::Uuid::parse_str(collection).is_ok() {
                    params.push(collection.to_string());
                    sql.push_str(&format!(
                        " AND n.collection_id = ${}::uuid",
                        params.len() + 1
                    ));
                }
            } else if let Some(ts) = token.strip_prefix("created_after:") {
                if chrono::DateTime::parse_from_rfc3339(ts).is_ok() {
                    params.push(ts.to_string());
                    sql.push_str(&format!(
                        " AND n.created_at_utc >= ${}::timestamptz",
                        params.len() + 1
                    ));
                }
            } else if let Some(ts) = token.strip_prefix("created_before:") {
                if chrono::DateTime::parse_from_rfc3339(ts).is_ok() {
                    params.push(ts.to_string());
                    sql.push_str(&format!(
                        " AND n.created_at_utc <= ${}::timestamptz",
                        params.len() + 1
                    ));
                }
            } else if let Some(ts) = token.strip_prefix("updated_after:") {
                if chrono::DateTime::parse_from_rfc3339(ts).is_ok() {
                    params.push(ts.to_string());
                    sql.push_str(&format!(
                        " AND n.updated_at_utc >= ${}::timestamptz",
                        params.len() + 1
                    ));
                }
            } else if let Some(ts) = token.strip_prefix("updated_before:") {
                if chrono::DateTime::parse_from_rfc3339(ts).is_ok() {
                    params.push(ts.to_string());
                    sql.push_str(&format!(
                        " AND n.updated_at_utc <= ${}::timestamptz",
                        params.len() + 1
                    ));
                }
            }
        }

        // If no filter conditions were added, return unfiltered results
        if params.is_empty() {
            return Ok(results);
        }

        let mut q = sqlx::query_scalar::<_, Uuid>(&sql);
        q = q.bind(&note_ids);
        for param in &params {
            q = q.bind(param);
        }

        let matching_ids: std::collections::HashSet<Uuid> = q
            .fetch_all(self.db.pool())
            .await
            .map_err(matric_core::Error::Database)?
            .into_iter()
            .collect();

        Ok(results
            .into_iter()
            .filter(|hit| matching_ids.contains(&hit.note_id))
            .collect())
    }

    /// Post-filter a set of note IDs by strict tag filter (fixes #235, #236).
    ///
    /// Used for non-English FTS strategies and the search_filtered path where
    /// strict_filter cannot be applied server-side in the FTS query.
    async fn filter_notes_by_strict_filter(
        &self,
        note_ids: &[Uuid],
        filter: &matric_core::StrictTagFilter,
    ) -> Result<std::collections::HashSet<Uuid>> {
        use matric_db::strict_filter::StrictFilterQueryBuilder;

        let builder = StrictFilterQueryBuilder::new(filter.clone(), 1);
        let (filter_clause, filter_params) = builder.build();

        let sql = format!(
            "SELECT n.id FROM note n WHERE n.id = ANY($1::uuid[]) AND {}",
            filter_clause
        );

        let mut q = sqlx::query_scalar::<_, Uuid>(&sql);
        q = q.bind(note_ids);

        for param in &filter_params {
            q = match param {
                matric_db::strict_filter::QueryParam::Uuid(id) => q.bind(id),
                matric_db::strict_filter::QueryParam::UuidArray(ids) => q.bind(ids),
                matric_db::strict_filter::QueryParam::Int(val) => q.bind(val),
                matric_db::strict_filter::QueryParam::Timestamp(ts) => q.bind(ts),
                matric_db::strict_filter::QueryParam::Bool(b) => q.bind(b),
                matric_db::strict_filter::QueryParam::String(s) => q.bind(s),
                matric_db::strict_filter::QueryParam::StringArray(arr) => q.bind(arr),
            };
        }

        let ids = q
            .fetch_all(self.db.pool())
            .await
            .map_err(matric_core::Error::Database)?;
        Ok(ids.into_iter().collect())
    }
}

#[async_trait]
impl HybridSearch for HybridSearchEngine {
    #[instrument(skip(self, query_embedding, config), fields(
        subsystem = "search",
        component = "hybrid_search",
        op = "search",
        query = %query,
        fts_weight = config.fts_weight,
        semantic_weight = config.semantic_weight,
    ))]
    async fn search(
        &self,
        query: &str,
        query_embedding: Option<&Vector>,
        limit: i64,
        config: &HybridSearchConfig,
    ) -> Result<Vec<EnhancedSearchHit>> {
        let start = Instant::now();
        let mut ranked_lists = Vec::new();
        let mut fts_count = 0usize;
        let mut semantic_count = 0usize;

        // Select search strategy based on script detection
        let (strategy, detected_script) = Self::select_strategy(query, config);
        debug!(
            ?detected_script,
            ?strategy,
            "Selected search strategy based on script detection"
        );

        // FTS search (if weight > 0 and query is not empty)
        if config.fts_weight > 0.0 && !query.trim().is_empty() {
            let fts_start = Instant::now();
            let mut fts_results = self
                .fts_search_with_strategy(query, strategy, limit * 2, config)
                .await?;

            // Filter FTS results to embedding set members (issue #125)
            if let Some(set_id) = config.embedding_set_id {
                let member_ids = self.get_set_member_ids(set_id).await?;
                fts_results.retain(|hit| member_ids.contains(&hit.note_id));
            }

            fts_count = fts_results.len();
            debug!(
                fts_hits = fts_count,
                ?strategy,
                duration_ms = fts_start.elapsed().as_millis() as u64,
                "FTS retrieval complete"
            );

            if !fts_results.is_empty() {
                ranked_lists.push(Self::apply_weights(fts_results, config.fts_weight));
            }
        }

        // Semantic search (if weight > 0 and embedding is provided)
        if config.semantic_weight > 0.0 {
            if let Some(embedding) = query_embedding {
                let sem_start = Instant::now();
                // Apply strict filter, embedding set, or search all embeddings
                let semantic_results = if let Some(ref strict_filter) = config.strict_filter {
                    // Strict filter takes priority - ensures data isolation
                    self.db
                        .embeddings
                        .find_similar_with_strict_filter(
                            embedding,
                            strict_filter,
                            limit * 2,
                            config.exclude_archived,
                        )
                        .await?
                } else if let Some(set_id) = config.embedding_set_id {
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
                // Filter out low-similarity semantic results BEFORE RRF fusion (fixes #384).
                // Without this, nonsense queries return results because semantic search
                // always returns top-K regardless of similarity, and RRF normalization
                // with a single list inflates scores to near 1.0.
                let semantic_results: Vec<SearchHit> = semantic_results
                    .into_iter()
                    .filter(|hit| hit.score >= MIN_SEMANTIC_SIMILARITY)
                    .collect();

                semantic_count = semantic_results.len();
                debug!(
                    semantic_hits = semantic_count,
                    duration_ms = sem_start.elapsed().as_millis() as u64,
                    "Semantic retrieval complete"
                );

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
            debug!("No results from any source");
            return Ok(Vec::new());
        }

        // Fuse results using RRF (over-fetch to account for deduplication reducing count)
        let fusion_start = Instant::now();
        let mut results = rrf_fuse(ranked_lists, (limit as usize) * 3);
        debug!(
            fusion_method = "rrf",
            result_count = results.len(),
            duration_ms = fusion_start.elapsed().as_millis() as u64,
            "Fusion complete"
        );

        // Apply minimum score filter
        if config.min_score > 0.0 {
            results.retain(|hit| hit.score >= config.min_score);
        }

        // Apply deduplication, then enforce requested limit (fixes #183)
        let mut deduplicated = deduplicate_search_results(results, &config.deduplication);
        deduplicated.truncate(limit as usize);

        info!(
            fts_hits = fts_count,
            semantic_hits = semantic_count,
            result_count = deduplicated.len(),
            duration_ms = start.elapsed().as_millis() as u64,
            "Hybrid search completed"
        );

        Ok(deduplicated)
    }

    #[instrument(skip(self, query_embedding, config), fields(
        subsystem = "search",
        component = "hybrid_search",
        op = "search_filtered",
        query = %query,
    ))]
    async fn search_filtered(
        &self,
        query: &str,
        query_embedding: Option<&Vector>,
        filters: &str,
        limit: i64,
        config: &HybridSearchConfig,
    ) -> Result<Vec<EnhancedSearchHit>> {
        let start = Instant::now();
        let mut ranked_lists = Vec::new();

        // FTS search with filters — strategy-aware (fixes #295/#288 emoji search)
        if config.fts_weight > 0.0 && !query.trim().is_empty() {
            let (strategy, _script) = Self::select_strategy(query, config);
            let mut fts_results = if strategy == SearchStrategy::FtsEnglish {
                // English FTS can efficiently combine with SQL-level filters
                self.db
                    .search
                    .search_filtered(query, filters, limit * 2, config.exclude_archived)
                    .await?
            } else {
                // Non-English strategies (trigram, bigram, CJK): use strategy-aware
                // FTS then post-filter by query-level filters
                let unfiltered = self
                    .fts_search_with_strategy(query, strategy, limit * 4, config)
                    .await?;
                self.post_filter_by_query_filters(unfiltered, filters, config.exclude_archived)
                    .await?
            };

            // Apply strict_filter post-filtering (fixes #235 — search_filtered path)
            if let Some(ref strict_filter) = config.strict_filter {
                if strict_filter.match_none {
                    fts_results.clear();
                } else if !strict_filter.is_empty() {
                    let note_ids: Vec<Uuid> = fts_results.iter().map(|h| h.note_id).collect();
                    if !note_ids.is_empty() {
                        let matching = self
                            .filter_notes_by_strict_filter(&note_ids, strict_filter)
                            .await?;
                        fts_results.retain(|hit| matching.contains(&hit.note_id));
                    }
                }
            }

            // Filter FTS results to embedding set members (fixes #237)
            if let Some(set_id) = config.embedding_set_id {
                let member_ids = self.get_set_member_ids(set_id).await?;
                fts_results.retain(|hit| member_ids.contains(&hit.note_id));
            }

            debug!(fts_hits = fts_results.len(), "FTS filtered retrieval");

            if !fts_results.is_empty() {
                ranked_lists.push(Self::apply_weights(fts_results, config.fts_weight));
            }
        }

        // Semantic search with optional strict filter for data isolation
        if config.semantic_weight > 0.0 {
            if let Some(embedding) = query_embedding {
                // Apply strict filter, embedding set, or search all embeddings
                let semantic_results = if let Some(ref strict_filter) = config.strict_filter {
                    // Strict filter takes priority - ensures data isolation
                    self.db
                        .embeddings
                        .find_similar_with_strict_filter(
                            embedding,
                            strict_filter,
                            limit * 2,
                            config.exclude_archived,
                        )
                        .await?
                } else if let Some(set_id) = config.embedding_set_id {
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
                // Filter out low-similarity semantic results before RRF fusion (fixes #384)
                let semantic_results: Vec<SearchHit> = semantic_results
                    .into_iter()
                    .filter(|hit| hit.score >= MIN_SEMANTIC_SIMILARITY)
                    .collect();

                debug!(semantic_hits = semantic_results.len(), "Semantic retrieval");

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

        let mut results = rrf_fuse(ranked_lists, (limit as usize) * 3);

        if config.min_score > 0.0 {
            results.retain(|hit| hit.score >= config.min_score);
        }

        // Apply deduplication, then enforce requested limit (fixes #183)
        let mut deduplicated = deduplicate_search_results(results, &config.deduplication);
        deduplicated.truncate(limit as usize);

        info!(
            result_count = deduplicated.len(),
            duration_ms = start.elapsed().as_millis() as u64,
            "Filtered hybrid search completed"
        );

        Ok(deduplicated)
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
    pub config: HybridSearchConfig,
    /// Filter: notes created after this timestamp
    created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes created before this timestamp
    created_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes updated after this timestamp
    updated_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Filter: notes updated before this timestamp
    updated_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Sort field: "relevance", "created_at", "updated_at", "title"
    sort_by: Option<String>,
    /// Sort order: "asc" or "desc"
    sort_order: Option<String>,
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
            updated_after: None,
            updated_before: None,
            sort_by: None,
            sort_order: None,
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

    /// Filter to notes updated after this timestamp.
    pub fn with_updated_after(mut self, ts: chrono::DateTime<chrono::Utc>) -> Self {
        self.updated_after = Some(ts);
        self
    }

    /// Filter to notes updated before this timestamp.
    pub fn with_updated_before(mut self, ts: chrono::DateTime<chrono::Utc>) -> Self {
        self.updated_before = Some(ts);
        self
    }

    /// Set the sort field (relevance, created_at, updated_at, title).
    pub fn with_sort_by(mut self, sort_by: impl Into<String>) -> Self {
        self.sort_by = Some(sort_by.into());
        self
    }

    /// Set the sort order (asc or desc).
    pub fn with_sort_order(mut self, order: impl Into<String>) -> Self {
        self.sort_order = Some(order.into());
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

    /// Enable or disable deduplication.
    pub fn with_deduplication(mut self, enabled: bool) -> Self {
        self.config.deduplication.deduplicate_chains = enabled;
        self
    }

    /// Set strict tag filter for taxonomy-based filtering.
    pub fn with_strict_filter(mut self, filter: StrictTagFilter) -> Self {
        self.config.strict_filter = Some(filter);
        self
    }

    /// Execute the search request.
    pub async fn execute(self, engine: &HybridSearchEngine) -> Result<Vec<EnhancedSearchHit>> {
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
        if let Some(ts) = &self.updated_after {
            filter_parts.push(format!("updated_after:{}", ts.to_rfc3339()));
        }
        if let Some(ts) = &self.updated_before {
            filter_parts.push(format!("updated_before:{}", ts.to_rfc3339()));
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
        assert!(config.deduplication.deduplicate_chains);
        assert!(!config.deduplication.expand_chains);
        assert!(config.strict_filter.is_none());
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
                embedding_status: None,
            },
            SearchHit {
                note_id: Uuid::new_v4(),
                score: 0.5,
                snippet: None,
                title: None,
                tags: Vec::new(),
                embedding_status: None,
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

    #[test]
    fn test_config_with_strict_filter() {
        let filter = StrictTagFilter::new()
            .require_concept(Uuid::new_v4())
            .any_concept(Uuid::new_v4());

        let config = HybridSearchConfig::default().with_strict_filter(filter.clone());
        assert!(config.strict_filter.is_some());
        let stored_filter = config.strict_filter.unwrap();
        assert_eq!(stored_filter.required_concepts.len(), 1);
        assert_eq!(stored_filter.any_concepts.len(), 1);
    }

    #[test]
    fn test_search_request_with_strict_filter() {
        let filter = StrictTagFilter::new()
            .exclude_concept(Uuid::new_v4())
            .with_min_tag_count(3);

        let request = SearchRequest::new("test").with_strict_filter(filter.clone());
        assert!(request.config.strict_filter.is_some());
        let stored_filter = request.config.strict_filter.unwrap();
        assert_eq!(stored_filter.excluded_concepts.len(), 1);
        assert_eq!(stored_filter.min_tag_count, Some(3));
    }

    // ========== DEDUPLICATION TESTS ==========

    #[test]
    fn test_config_with_deduplication() {
        let dedup_config = DeduplicationConfig {
            deduplicate_chains: false,
            expand_chains: true,
        };
        let config = HybridSearchConfig::default().with_deduplication(dedup_config.clone());
        assert_eq!(
            config.deduplication.deduplicate_chains,
            dedup_config.deduplicate_chains
        );
        assert_eq!(
            config.deduplication.expand_chains,
            dedup_config.expand_chains
        );
    }

    #[test]
    fn test_config_with_deduplication_enabled() {
        let config = HybridSearchConfig::default().with_deduplication_enabled(false);
        assert!(!config.deduplication.deduplicate_chains);

        let config2 = HybridSearchConfig::default().with_deduplication_enabled(true);
        assert!(config2.deduplication.deduplicate_chains);
    }

    #[test]
    fn test_config_with_chain_expansion() {
        let config = HybridSearchConfig::default().with_chain_expansion(true);
        assert!(config.deduplication.expand_chains);

        let config2 = HybridSearchConfig::default().with_chain_expansion(false);
        assert!(!config2.deduplication.expand_chains);
    }

    #[test]
    fn test_search_request_with_deduplication() {
        let request = SearchRequest::new("test").with_deduplication(false);
        assert!(!request.config.deduplication.deduplicate_chains);

        let request2 = SearchRequest::new("test").with_deduplication(true);
        assert!(request2.config.deduplication.deduplicate_chains);
    }

    #[test]
    fn test_config_chaining_with_deduplication() {
        let set_id = Uuid::new_v4();
        let config = HybridSearchConfig::default()
            .with_min_score(0.3)
            .with_exclude_archived(false)
            .with_embedding_set(set_id)
            .with_deduplication_enabled(false)
            .with_chain_expansion(true);

        assert_eq!(config.min_score, 0.3);
        assert!(!config.exclude_archived);
        assert_eq!(config.embedding_set_id, Some(set_id));
        assert!(!config.deduplication.deduplicate_chains);
        assert!(config.deduplication.expand_chains);
    }

    #[test]
    fn test_config_chaining_with_strict_filter() {
        let set_id = Uuid::new_v4();
        let filter = StrictTagFilter::new()
            .require_concept(Uuid::new_v4())
            .exclude_concept(Uuid::new_v4());

        let config = HybridSearchConfig::default()
            .with_min_score(0.3)
            .with_exclude_archived(false)
            .with_embedding_set(set_id)
            .with_deduplication_enabled(false)
            .with_chain_expansion(true)
            .with_strict_filter(filter.clone());

        assert_eq!(config.min_score, 0.3);
        assert!(!config.exclude_archived);
        assert_eq!(config.embedding_set_id, Some(set_id));
        assert!(!config.deduplication.deduplicate_chains);
        assert!(config.deduplication.expand_chains);
        assert!(config.strict_filter.is_some());
    }

    // ========== EXISTING COMPREHENSIVE TESTS ==========

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
            embedding_status: None,
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
            embedding_status: None,
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
            embedding_status: None,
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
