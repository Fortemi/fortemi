//! ColBERT Late Interaction Re-ranking
//!
//! Implements ColBERT-style token-level embeddings for fine-grained semantic matching.
//!
//! # Architecture
//!
//! ColBERT (Contextualized Late Interaction over BERT) enables precise semantic matching by:
//! 1. Storing per-token 128-dim embeddings for documents
//! 2. At query time, encoding query tokens to 128-dim embeddings
//! 3. Computing MaxSim score: Σ max(qi · dj) for all query tokens i and doc tokens j
//!
//! # Usage
//!
//! ```ignore
//! use matric_search::ColBERTReranker;
//!
//! let reranker = ColBERTReranker::new(db);
//!
//! // Re-rank initial search results
//! let reranked = reranker.rerank(
//!     initial_results,
//!     query_tokens,
//!     top_k: 20
//! ).await?;
//! ```

use pgvector::Vector;
use uuid::Uuid;

use matric_core::{Error, Result, SearchHit};
use matric_db::Database;
use matric_db::TokenEmbedding;

/// Configuration for ColBERT re-ranking.
#[derive(Debug, Clone)]
pub struct ColBERTConfig {
    /// Number of top candidates to re-rank (from initial retrieval)
    pub top_k: usize,
    /// Whether to enable ColBERT re-ranking
    pub enabled: bool,
    /// Minimum MaxSim score threshold (0.0 to disable)
    pub min_score: f32,
}

impl Default for ColBERTConfig {
    fn default() -> Self {
        Self {
            top_k: 100,
            enabled: false,
            min_score: 0.0,
        }
    }
}

impl ColBERTConfig {
    /// Create a new config with re-ranking enabled.
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// Set the number of candidates to re-rank.
    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = k;
        self
    }

    /// Set minimum score threshold.
    pub fn with_min_score(mut self, score: f32) -> Self {
        self.min_score = score;
        self
    }
}

/// ColBERT late interaction re-ranker.
pub struct ColBERTReranker {
    db: Database,
}

impl ColBERTReranker {
    /// Create a new ColBERT re-ranker.
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Compute MaxSim score between query tokens and document tokens.
    ///
    /// MaxSim = Σ max(qi · dj) for all query tokens i and doc tokens j
    ///
    /// For each query token, find the most similar document token (max cosine similarity),
    /// then sum these maximum similarities across all query tokens.
    pub fn compute_maxsim(query_tokens: &[Vector], doc_tokens: &[TokenEmbedding]) -> Result<f32> {
        if query_tokens.is_empty() || doc_tokens.is_empty() {
            return Ok(0.0);
        }

        let mut total_score = 0.0;

        // For each query token
        for q_token in query_tokens {
            let mut max_sim = f32::NEG_INFINITY;

            // Find max similarity with any document token
            for d_token in doc_tokens {
                let sim = Self::cosine_similarity(q_token, &d_token.embedding)?;
                if sim > max_sim {
                    max_sim = sim;
                }
            }

            total_score += max_sim;
        }

        Ok(total_score)
    }

    /// Compute cosine similarity between two vectors.
    fn cosine_similarity(a: &Vector, b: &Vector) -> Result<f32> {
        let a_vec = a.as_slice();
        let b_vec = b.as_slice();

        if a_vec.len() != b_vec.len() {
            return Err(Error::InvalidInput(format!(
                "Vector dimension mismatch: {} != {}",
                a_vec.len(),
                b_vec.len()
            )));
        }

        let dot_product: f32 = a_vec.iter().zip(b_vec.iter()).map(|(x, y)| x * y).sum();

        let a_norm: f32 = a_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        let b_norm: f32 = b_vec.iter().map(|x| x * x).sum::<f32>().sqrt();

        if a_norm == 0.0 || b_norm == 0.0 {
            return Ok(0.0);
        }

        Ok(dot_product / (a_norm * b_norm))
    }

    /// Re-rank search results using ColBERT MaxSim scoring.
    ///
    /// Takes initial search results and query token embeddings,
    /// retrieves document token embeddings, computes MaxSim scores,
    /// and returns re-ranked results.
    pub async fn rerank(
        &self,
        mut initial_results: Vec<SearchHit>,
        query_tokens: &[Vector],
        config: &ColBERTConfig,
    ) -> Result<Vec<SearchHit>> {
        if !config.enabled || query_tokens.is_empty() {
            return Ok(initial_results);
        }

        // Take top K candidates for re-ranking
        let candidates: Vec<SearchHit> = initial_results
            .drain(..config.top_k.min(initial_results.len()))
            .collect();

        // Re-rank each candidate
        let mut scored_results = Vec::with_capacity(candidates.len());

        for mut hit in candidates {
            // Retrieve token embeddings for this note
            let doc_tokens = self.db.colbert.get_token_embeddings(hit.note_id).await?;

            if doc_tokens.is_empty() {
                // No token embeddings available, keep original score
                scored_results.push(hit);
                continue;
            }

            // Compute MaxSim score
            let maxsim_score = Self::compute_maxsim(query_tokens, &doc_tokens)?;

            // Update hit score with MaxSim
            hit.score = maxsim_score;
            scored_results.push(hit);
        }

        // Sort by MaxSim score (descending)
        scored_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply minimum score filter
        if config.min_score > 0.0 {
            scored_results.retain(|hit| hit.score >= config.min_score);
        }

        Ok(scored_results)
    }

    /// Check if a note has ColBERT token embeddings.
    pub async fn has_embeddings(&self, note_id: Uuid) -> Result<bool> {
        self.db.colbert.has_embeddings(note_id).await
    }

    /// Get token count for a note.
    pub async fn get_token_count(&self, note_id: Uuid) -> Result<i32> {
        self.db.colbert.get_token_count(note_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a vector from a slice
    fn vec_from_slice(data: &[f32]) -> Vector {
        Vector::from(data.to_vec())
    }

    // Helper to create token embeddings
    fn create_token(pos: i32, text: &str, embedding: Vec<f32>) -> TokenEmbedding {
        TokenEmbedding {
            id: Uuid::new_v4(),
            note_id: Uuid::new_v4(),
            chunk_id: None,
            token_position: pos,
            model: "colbert-v2".to_string(),
            token_text: text.to_string(),
            embedding: Vector::from(embedding),
        }
    }

    #[test]
    fn test_colbert_config_default() {
        let config = ColBERTConfig::default();
        assert_eq!(config.top_k, 100);
        assert!(!config.enabled);
        assert_eq!(config.min_score, 0.0);
    }

    #[test]
    fn test_colbert_config_enabled() {
        let config = ColBERTConfig::enabled();
        assert!(config.enabled);
    }

    #[test]
    fn test_colbert_config_builder() {
        let config = ColBERTConfig::enabled().with_top_k(50).with_min_score(0.5);

        assert!(config.enabled);
        assert_eq!(config.top_k, 50);
        assert_eq!(config.min_score, 0.5);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec_from_slice(&[1.0, 0.0, 0.0]);
        let b = vec_from_slice(&[1.0, 0.0, 0.0]);

        let sim = ColBERTReranker::cosine_similarity(&a, &b).unwrap();
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec_from_slice(&[1.0, 0.0, 0.0]);
        let b = vec_from_slice(&[0.0, 1.0, 0.0]);

        let sim = ColBERTReranker::cosine_similarity(&a, &b).unwrap();
        assert!((sim - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec_from_slice(&[1.0, 0.0, 0.0]);
        let b = vec_from_slice(&[-1.0, 0.0, 0.0]);

        let sim = ColBERTReranker::cosine_similarity(&a, &b).unwrap();
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_dimension_mismatch() {
        let a = vec_from_slice(&[1.0, 0.0]);
        let b = vec_from_slice(&[1.0, 0.0, 0.0]);

        let result = ColBERTReranker::cosine_similarity(&a, &b);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidInput(_)));
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec_from_slice(&[0.0, 0.0, 0.0]);
        let b = vec_from_slice(&[1.0, 0.0, 0.0]);

        let sim = ColBERTReranker::cosine_similarity(&a, &b).unwrap();
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_compute_maxsim_empty_query() {
        let query_tokens: Vec<Vector> = vec![];
        let doc_tokens = vec![create_token(0, "test", vec![1.0, 0.0, 0.0])];

        let score = ColBERTReranker::compute_maxsim(&query_tokens, &doc_tokens).unwrap();
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_compute_maxsim_empty_doc() {
        let query_tokens = vec![vec_from_slice(&[1.0, 0.0, 0.0])];
        let doc_tokens: Vec<TokenEmbedding> = vec![];

        let score = ColBERTReranker::compute_maxsim(&query_tokens, &doc_tokens).unwrap();
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_compute_maxsim_single_perfect_match() {
        let query_tokens = vec![vec_from_slice(&[1.0, 0.0, 0.0])];
        let doc_tokens = vec![create_token(0, "test", vec![1.0, 0.0, 0.0])];

        let score = ColBERTReranker::compute_maxsim(&query_tokens, &doc_tokens).unwrap();
        assert!((score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_maxsim_multiple_query_tokens() {
        // Query: "machine learning"
        let query_tokens = vec![
            vec_from_slice(&[1.0, 0.0, 0.0]), // "machine"
            vec_from_slice(&[0.0, 1.0, 0.0]), // "learning"
        ];

        // Document has matching tokens
        let doc_tokens = vec![
            create_token(0, "machine", vec![1.0, 0.0, 0.0]), // exact match
            create_token(1, "learning", vec![0.0, 1.0, 0.0]), // exact match
            create_token(2, "other", vec![0.0, 0.0, 1.0]),   // not matched
        ];

        let score = ColBERTReranker::compute_maxsim(&query_tokens, &doc_tokens).unwrap();
        // Score = 1.0 (machine) + 1.0 (learning) = 2.0
        assert!((score - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_maxsim_partial_match() {
        // Query token that partially matches multiple doc tokens
        let query_tokens = vec![vec_from_slice(&[0.5, 0.5, 0.0]).normalize()];

        let doc_tokens = vec![
            create_token(0, "token1", vec![1.0, 0.0, 0.0]),
            create_token(1, "token2", vec![0.0, 1.0, 0.0]),
        ];

        let score = ColBERTReranker::compute_maxsim(&query_tokens, &doc_tokens).unwrap();
        // Should pick the max similarity (which should be > 0)
        assert!(score > 0.0);
    }

    #[test]
    fn test_compute_maxsim_finds_best_match_per_query_token() {
        let query_tokens = vec![vec_from_slice(&[1.0, 0.0, 0.0])];

        // Document has multiple tokens, should pick best match
        let doc_tokens = vec![
            create_token(0, "bad", vec![0.0, 1.0, 0.0]), // similarity = 0
            create_token(1, "medium", vec![0.5, 0.5, 0.0]), // similarity < 1
            create_token(2, "perfect", vec![1.0, 0.0, 0.0]), // similarity = 1
        ];

        let score = ColBERTReranker::compute_maxsim(&query_tokens, &doc_tokens).unwrap();
        // Should pick the perfect match (score = 1.0)
        assert!((score - 1.0).abs() < 0.1); // Allow some tolerance for normalization
    }

    #[test]
    fn test_compute_maxsim_aggregates_across_query_tokens() {
        // Multiple query tokens should aggregate their max similarities
        let query_tokens = vec![
            vec_from_slice(&[1.0, 0.0, 0.0]),
            vec_from_slice(&[0.0, 1.0, 0.0]),
            vec_from_slice(&[0.0, 0.0, 1.0]),
        ];

        let doc_tokens = vec![
            create_token(0, "x", vec![1.0, 0.0, 0.0]),
            create_token(1, "y", vec![0.0, 1.0, 0.0]),
            create_token(2, "z", vec![0.0, 0.0, 1.0]),
        ];

        let score = ColBERTReranker::compute_maxsim(&query_tokens, &doc_tokens).unwrap();
        // Score = 1.0 + 1.0 + 1.0 = 3.0
        assert!((score - 3.0).abs() < 1e-6);
    }

    // Helper trait to normalize vectors for testing
    trait Normalize {
        fn normalize(self) -> Self;
    }

    impl Normalize for Vector {
        fn normalize(self) -> Self {
            let vec = self.as_slice();
            let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm == 0.0 {
                return self;
            }
            Vector::from(vec.iter().map(|x| x / norm).collect::<Vec<f32>>())
        }
    }

    // Integration tests will be in matric-db/tests/colbert_tests.rs
}
