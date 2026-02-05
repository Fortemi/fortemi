//! Test fixtures for embedding coverage tests.
//!
//! Provides reusable test data for integration tests.

use uuid::Uuid;

/// Sample note content for quantum computing theme.
pub mod quantum_computing {
    pub const NOTE1: &str = "Quantum computing leverages quantum bits (qubits) to perform \
        computations. Unlike classical bits, qubits can exist in superposition states.";

    pub const NOTE2: &str = "Quantum entanglement is a fundamental property in quantum mechanics \
        where particles become correlated. This is crucial for quantum computing.";

    pub const NOTE3: &str = "Quantum algorithms like Shor's algorithm can factor large numbers \
        exponentially faster than classical algorithms.";

    pub const NOTE4: &str = "Quantum error correction is essential for building fault-tolerant \
        quantum computers that can perform long computations reliably.";

    pub const QUERY_SIMILAR: &str = "quantum physics and superposition";
    pub const QUERY_EXACT: &str = "quantum computing";
}

/// Sample note content for machine learning theme.
pub mod machine_learning {
    pub const NOTE_ML1: &str = "Machine learning is a subset of artificial intelligence focused \
        on learning from data.";

    pub const NOTE_ML2: &str = "Neural networks are computational models inspired by biological \
        neurons in the brain.";

    pub const NOTE_ML3: &str = "Deep learning uses multi-layer neural networks to learn \
        hierarchical representations.";

    pub const NOTE_ML4: &str = "Supervised learning algorithms learn from labeled training data \
        to make predictions.";

    pub const NOTE_ML5: &str = "Unsupervised learning discovers patterns in unlabeled data \
        without explicit guidance.";

    pub const NOTE_ML6: &str = "Reinforcement learning trains agents to make decisions through \
        trial and error.";

    pub const NOTE_ML7: &str = "Transfer learning reuses pre-trained models on related tasks to \
        improve performance.";

    pub const NOTE_ML8: &str = "Convolutional neural networks excel at computer vision tasks \
        like image classification.";

    pub const NOTE_ML9: &str = "Recurrent neural networks process sequential data like text and \
        time series.";

    pub const NOTE_ML10: &str = "Attention mechanisms allow models to focus on relevant parts of \
        the input during processing.";

    pub const QUERY: &str = "machine learning";
}

/// Sample notes with semantic similarity but lexical differences.
pub mod hybrid_search_scenario {
    /// Embedded note with exact lexical match
    pub const NOTE_A: (&str, bool) = (
        "Neural networks and deep learning architectures for computer vision",
        true, // embedded
    );

    /// Embedded note with semantic similarity
    pub const NOTE_B: (&str, bool) = (
        "Machine learning algorithms for pattern recognition and classification",
        true, // embedded
    );

    /// Non-embedded note with lexical match
    pub const NOTE_C: (&str, bool) = (
        "Artificial intelligence research methods and applications",
        false, // not embedded
    );

    /// Non-embedded note with abbreviation
    pub const NOTE_D: (&str, bool) = (
        "AI and neural nets discussion forum for researchers",
        false, // not embedded
    );

    pub const QUERY_DEEP_LEARNING: &str = "deep learning";
    pub const QUERY_ARTIFICIAL_INTELLIGENCE: &str = "artificial intelligence";
}

/// Test embedding set configurations.
pub struct EmbeddingSetFixture {
    pub name: String,
    pub slug: String,
    pub description: String,
    pub mode: String,
}

impl EmbeddingSetFixture {
    pub fn manual_set() -> Self {
        Self {
            name: "Test Manual Set".to_string(),
            slug: "test-manual".to_string(),
            description: "Manual embedding set for testing coverage".to_string(),
            mode: "manual".to_string(),
        }
    }

    pub fn auto_set_with_tag_filter(tag: &str) -> Self {
        Self {
            name: format!("Auto Set (tag:{})", tag),
            slug: format!("auto-tag-{}", tag.to_lowercase()),
            description: format!("Auto-embed set for documents tagged with '{}'", tag),
            mode: "auto".to_string(),
        }
    }

    pub fn filter_set_with_parent(parent_set_id: uuid::Uuid) -> Self {
        Self {
            name: format!("Filter Set (parent: {})", parent_set_id),
            slug: format!("filter-{}", uuid::Uuid::new_v4()),
            description: "Filter set sharing embeddings from parent".to_string(),
            mode: "manual".to_string(),
        }
    }
}

/// Coverage threshold definitions for warning logic.
pub struct CoverageThresholds;

impl CoverageThresholds {
    /// Coverage below this triggers "very low coverage" warning
    pub const VERY_LOW: f64 = 25.0;

    /// Coverage below this triggers "low coverage" warning
    pub const LOW: f64 = 50.0;

    /// Coverage below this triggers "medium coverage" info
    pub const MEDIUM: f64 = 75.0;

    /// Coverage at or above this is considered complete
    pub const COMPLETE: f64 = 100.0;

    pub fn warning_level(coverage_pct: f64) -> &'static str {
        if coverage_pct == 0.0 {
            "empty"
        } else if coverage_pct < Self::VERY_LOW {
            "very_low"
        } else if coverage_pct < Self::LOW {
            "low"
        } else if coverage_pct < Self::MEDIUM {
            "medium"
        } else if coverage_pct < Self::COMPLETE {
            "high"
        } else {
            "complete"
        }
    }
}

/// Test note builder for creating test documents.
pub struct TestNoteBuilder {
    pub title: String,
    pub content: String,
    pub tags: Vec<Uuid>,
    pub note_type: String,
}

impl TestNoteBuilder {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            content: content.into(),
            tags: Vec::new(),
            note_type: "markdown".to_string(),
        }
    }

    pub fn with_tag(mut self, tag_id: Uuid) -> Self {
        self.tags.push(tag_id);
        self
    }

    pub fn with_type(mut self, note_type: impl Into<String>) -> Self {
        self.note_type = note_type.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coverage_threshold_levels() {
        assert_eq!(CoverageThresholds::warning_level(0.0), "empty");
        assert_eq!(CoverageThresholds::warning_level(10.0), "very_low");
        assert_eq!(CoverageThresholds::warning_level(30.0), "low");
        assert_eq!(CoverageThresholds::warning_level(60.0), "medium");
        assert_eq!(CoverageThresholds::warning_level(80.0), "high");
        assert_eq!(CoverageThresholds::warning_level(100.0), "complete");
    }

    #[test]
    fn test_fixture_manual_set_creation() {
        let fixture = EmbeddingSetFixture::manual_set();
        assert_eq!(fixture.mode, "manual");
        assert_eq!(fixture.slug, "test-manual");
    }

    #[test]
    fn test_fixture_auto_set_creation() {
        let fixture = EmbeddingSetFixture::auto_set_with_tag_filter("tutorial");
        assert_eq!(fixture.mode, "auto");
        assert!(fixture.slug.contains("tutorial"));
    }

    #[test]
    fn test_note_builder() {
        let note = TestNoteBuilder::new("Test Note", "Content here")
            .with_type("code")
            .with_tag(Uuid::new_v4());

        assert_eq!(note.title, "Test Note");
        assert_eq!(note.content, "Content here");
        assert_eq!(note.note_type, "code");
        assert_eq!(note.tags.len(), 1);
    }
}
