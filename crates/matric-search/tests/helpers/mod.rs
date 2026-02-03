//! Test helpers for embedding coverage tests.
//!
//! Provides utilities for database setup, mocking, and assertions.

use matric_core::EmbeddingIndexStatus;

/// Helper for asserting coverage statistics.
pub struct CoverageAssertion {
    pub document_count: i32,
    pub embedding_count: i32,
    pub expected_coverage_pct: f64,
    pub tolerance: f64,
}

impl CoverageAssertion {
    pub fn new(document_count: i32, embedding_count: i32) -> Self {
        Self {
            document_count,
            embedding_count,
            expected_coverage_pct: (embedding_count as f64 / document_count as f64) * 100.0,
            tolerance: 0.01, // 0.01% tolerance for floating point
        }
    }

    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    pub fn assert(&self) {
        let actual_coverage =
            (self.embedding_count as f64 / self.document_count as f64) * 100.0;
        let diff = (actual_coverage - self.expected_coverage_pct).abs();
        assert!(
            diff < self.tolerance,
            "Coverage mismatch: expected {:.2}%, got {:.2}% (diff: {:.4}%)",
            self.expected_coverage_pct,
            actual_coverage,
            diff
        );
    }
}

/// Helper for asserting index status transitions.
pub struct StatusTransitionAssertion {
    pub from: EmbeddingIndexStatus,
    pub to: EmbeddingIndexStatus,
}

impl StatusTransitionAssertion {
    pub fn new(from: EmbeddingIndexStatus, to: EmbeddingIndexStatus) -> Self {
        Self { from, to }
    }

    /// Check if transition is valid according to state machine rules.
    pub fn is_valid_transition(&self) -> bool {
        use EmbeddingIndexStatus::*;

        matches!(
            (self.from, self.to),
            // Valid transitions
            (Empty, Pending)
                | (Pending, Building)
                | (Building, Ready)
                | (Building, Stale) // If new docs added during build
                | (Ready, Stale)
                | (Stale, Building)
                | (Stale, Ready)
                | (_, Disabled) // Any status can transition to disabled
                | (Disabled, Pending) // Re-enable
        )
    }

    pub fn assert_valid(&self) {
        assert!(
            self.is_valid_transition(),
            "Invalid status transition: {:?} -> {:?}",
            self.from,
            self.to
        );
    }

    pub fn assert_invalid(&self) {
        assert!(
            !self.is_valid_transition(),
            "Expected invalid transition but {:?} -> {:?} is valid",
            self.from,
            self.to
        );
    }
}

/// Helper for comparing search result counts.
pub struct SearchResultComparison {
    pub fts_count: usize,
    pub semantic_count: usize,
    pub hybrid_count: usize,
}

impl SearchResultComparison {
    pub fn new(fts_count: usize, semantic_count: usize, hybrid_count: usize) -> Self {
        Self {
            fts_count,
            semantic_count,
            hybrid_count,
        }
    }

    /// Assert that FTS returns at least as many results as semantic (when coverage < 100%).
    pub fn assert_fts_gte_semantic(&self) {
        assert!(
            self.fts_count >= self.semantic_count,
            "FTS count ({}) should be >= semantic count ({}) for partial coverage",
            self.fts_count,
            self.semantic_count
        );
    }

    /// Assert that hybrid includes results from both FTS and semantic.
    pub fn assert_hybrid_includes_both(&self) {
        assert!(
            self.hybrid_count >= self.fts_count,
            "Hybrid count ({}) should be >= FTS count ({})",
            self.hybrid_count,
            self.fts_count
        );
        assert!(
            self.hybrid_count >= self.semantic_count,
            "Hybrid count ({}) should be >= semantic count ({})",
            self.hybrid_count,
            self.semantic_count
        );
    }

    /// Assert that semantic returns zero when coverage is 0%.
    pub fn assert_semantic_empty_with_no_coverage(&self) {
        assert_eq!(
            self.semantic_count, 0,
            "Semantic search should return 0 results with 0% coverage"
        );
    }
}

/// Mock embedding vector generator for testing.
pub struct MockEmbeddingGenerator;

impl MockEmbeddingGenerator {
    /// Generate a deterministic mock embedding from text.
    /// Uses simple character-based hashing for reproducibility.
    pub fn generate(text: &str, dimension: usize) -> Vec<f32> {
        let mut vec = vec![0.0; dimension];
        for (i, c) in text.chars().enumerate() {
            let idx = (c as usize + i) % dimension;
            vec[idx] += 0.1;
        }
        // Normalize
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            vec.iter_mut().for_each(|x| *x /= magnitude);
        }
        vec
    }

    /// Generate a random-like embedding (still deterministic via seed).
    pub fn generate_with_seed(seed: u64, dimension: usize) -> Vec<f32> {
        let mut vec = vec![0.0; dimension];
        let mut state = seed;
        for i in 0..dimension {
            // Simple LCG for deterministic randomness
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            vec[i] = ((state % 1000) as f32) / 1000.0 - 0.5;
        }
        // Normalize
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            vec.iter_mut().for_each(|x| *x /= magnitude);
        }
        vec
    }
}

/// Helper for timing operations in performance tests.
pub struct Timer {
    start: std::time::Instant,
}

impl Timer {
    pub fn start() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    pub fn assert_under_ms(&self, threshold_ms: u128, operation: &str) {
        let elapsed = self.elapsed_ms();
        assert!(
            elapsed < threshold_ms,
            "{} took {}ms, expected < {}ms",
            operation,
            elapsed,
            threshold_ms
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coverage_assertion() {
        let assertion = CoverageAssertion::new(100, 75);
        assert!((assertion.expected_coverage_pct - 75.0).abs() < 0.01);
        assertion.assert();
    }

    #[test]
    fn test_coverage_assertion_with_tolerance() {
        let assertion = CoverageAssertion::new(100, 75).with_tolerance(1.0);
        assertion.assert();
    }

    #[test]
    #[should_panic]
    fn test_coverage_assertion_fails_on_mismatch() {
        let mut assertion = CoverageAssertion::new(100, 75);
        assertion.expected_coverage_pct = 50.0; // Mismatch
        assertion.tolerance = 0.01;
        assertion.assert();
    }

    #[test]
    fn test_valid_status_transitions() {
        use EmbeddingIndexStatus::*;

        // Valid transitions
        let valid_transitions = vec![
            (Empty, Pending),
            (Pending, Building),
            (Building, Ready),
            (Ready, Stale),
            (Stale, Building),
            (Stale, Ready),
            (Ready, Disabled),
            (Disabled, Pending),
        ];

        for (from, to) in valid_transitions {
            let assertion = StatusTransitionAssertion::new(from, to);
            assertion.assert_valid();
        }
    }

    #[test]
    fn test_invalid_status_transitions() {
        use EmbeddingIndexStatus::*;

        // Invalid transitions
        let invalid_transitions = vec![
            (Empty, Ready),     // Can't skip states
            (Pending, Ready),   // Can't skip building
            (Building, Empty),  // Can't go backwards
            (Ready, Pending),   // Can't regress
            (Ready, Building),  // Must go through Stale first
            (Empty, Stale),     // Nonsensical
        ];

        for (from, to) in invalid_transitions {
            let assertion = StatusTransitionAssertion::new(from, to);
            assertion.assert_invalid();
        }
    }

    #[test]
    fn test_search_result_comparison_fts_gte_semantic() {
        let comparison = SearchResultComparison::new(10, 5, 12);
        comparison.assert_fts_gte_semantic();
        comparison.assert_hybrid_includes_both();
    }

    #[test]
    #[should_panic]
    fn test_search_result_comparison_fails_when_semantic_greater() {
        let comparison = SearchResultComparison::new(5, 10, 12);
        comparison.assert_fts_gte_semantic();
    }

    #[test]
    fn test_search_result_zero_coverage() {
        let comparison = SearchResultComparison::new(10, 0, 10);
        comparison.assert_semantic_empty_with_no_coverage();
        comparison.assert_fts_gte_semantic();
    }

    #[test]
    fn test_mock_embedding_generator_deterministic() {
        let text = "quantum computing";
        let vec1 = MockEmbeddingGenerator::generate(text, 384);
        let vec2 = MockEmbeddingGenerator::generate(text, 384);

        assert_eq!(vec1.len(), 384);
        assert_eq!(vec1, vec2, "Should be deterministic");
    }

    #[test]
    fn test_mock_embedding_generator_normalized() {
        let vec = MockEmbeddingGenerator::generate("test", 128);
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.01, "Should be normalized");
    }

    #[test]
    fn test_mock_embedding_generator_with_seed() {
        let vec1 = MockEmbeddingGenerator::generate_with_seed(42, 256);
        let vec2 = MockEmbeddingGenerator::generate_with_seed(42, 256);
        let vec3 = MockEmbeddingGenerator::generate_with_seed(43, 256);

        assert_eq!(vec1, vec2, "Same seed should produce same vector");
        assert_ne!(vec1, vec3, "Different seed should produce different vector");
    }

    #[test]
    fn test_timer() {
        let timer = Timer::start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 10, "Should have elapsed at least 10ms");
    }
}
